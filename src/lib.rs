use pgrx::{pg_sys::panic::ErrorReportable, prelude::*};

::pgrx::pg_module_magic!();

#[pg_extern]
fn json_diff(left: pgrx::Json, right: pgrx::Json) -> serde_json::Result<pgrx::Json> {
    Ok(pgrx::Json(serde_json::to_value(json_patch::diff(
        &left.0, &right.0,
    ))?))
}

#[pg_extern]
fn jsonb_diff(left: pgrx::JsonB, right: pgrx::JsonB) -> serde_json::Result<pgrx::JsonB> {
    Ok(pgrx::JsonB(serde_json::to_value(json_patch::diff(
        &left.0, &right.0,
    ))?))
}

struct JsonPatch;

#[pg_aggregate]
impl Aggregate for JsonPatch {
    const NAME: &'static str = "json_patch";
    const INITIAL_CONDITION: Option<&'static str> = Some("{}");

    type State = pgrx::Json;
    type Args = pgrx::name!(value, pgrx::Json);

    #[pgrx(parallel_safe, immutable, strict)]
    fn state(
        mut current: Self::State,
        arg: Self::Args,
        _fcinfo: pg_sys::FunctionCallInfo,
    ) -> Self::State {
        // TODO: avoid clone?
        if let Ok(patch) = serde_json::from_value::<json_patch::Patch>(arg.0.clone()) {
            json_patch::patch(&mut current.0, &patch).unwrap_or_report();
        } else {
            json_patch::merge(&mut current.0, &arg.0);
        }
        current
    }
}

struct JsonBPatch;

#[pg_aggregate]
impl Aggregate for JsonBPatch {
    const NAME: &'static str = "jsonb_patch";
    const INITIAL_CONDITION: Option<&'static str> = Some("{}");

    type State = pgrx::JsonB;
    type Args = pgrx::name!(value, pgrx::JsonB);

    #[pgrx(parallel_safe, immutable, strict)]
    fn state(
        mut current: Self::State,
        arg: Self::Args,
        _fcinfo: pg_sys::FunctionCallInfo,
    ) -> Self::State {
        // TODO: avoid clone?
        if let Ok(patch) = serde_json::from_value::<json_patch::Patch>(arg.0.clone()) {
            json_patch::patch(&mut current.0, &patch).unwrap_or_report();
        } else {
            json_patch::merge(&mut current.0, &arg.0);
        }
        current
    }
}

struct JsonMerge;

#[pg_aggregate]
impl Aggregate for JsonMerge {
    const NAME: &'static str = "json_merge";
    const INITIAL_CONDITION: Option<&'static str> = Some("{}");

    type State = pgrx::Json;
    type Args = pgrx::name!(value, pgrx::Json);

    #[pgrx(parallel_safe, immutable, strict)]
    fn state(
        mut current: Self::State,
        arg: Self::Args,
        _fcinfo: pg_sys::FunctionCallInfo,
    ) -> Self::State {
        json_patch::merge(&mut current.0, &arg.0);
        current
    }
}

struct JsonBMerge;

#[pg_aggregate]
impl Aggregate for JsonBMerge {
    const NAME: &'static str = "jsonb_merge";
    const INITIAL_CONDITION: Option<&'static str> = Some("{}");

    type State = pgrx::JsonB;
    type Args = pgrx::name!(value, pgrx::JsonB);

    #[pgrx(parallel_safe, immutable, strict)]
    fn state(
        mut current: Self::State,
        arg: Self::Args,
        _fcinfo: pg_sys::FunctionCallInfo,
    ) -> Self::State {
        json_patch::merge(&mut current.0, &arg.0);
        current
    }
}

struct First;

#[pg_aggregate]
impl Aggregate for First {
    type State = pgrx::AnyElement;
    type Args = pgrx::name!(value, pgrx::AnyElement);

    #[pgrx(parallel_safe, immutable, strict)]
    fn state(
        current: Self::State,
        _arg: Self::Args,
        _fcinfo: pg_sys::FunctionCallInfo,
    ) -> Self::State {
        current
    }
}

struct Last;

#[pg_aggregate]
impl Aggregate for Last {
    type State = pgrx::AnyElement;
    type Args = pgrx::name!(value, pgrx::AnyElement);

    #[pgrx(parallel_safe, immutable, strict)]
    fn state(
        _current: Self::State,
        arg: Self::Args,
        _fcinfo: pg_sys::FunctionCallInfo,
    ) -> Self::State {
        arg
    }
}

#[pg_trigger]
fn trg_ins<'a>(
    trigger: &'a pgrx::PgTrigger<'a>,
) -> Result<Option<PgHeapTuple<'a, impl WhoAllocated>>, pgrx::PgTriggerError> {
    let tablename = trigger.table_name()?;

    // TODO: handle unwrap
    let mut new = trigger.new().map(|new| new.into_owned()).unwrap();
    let id = new.get_by_name::<pgrx::Uuid>("id");

    // TODO: handle unwrap
    let id = Spi::get_one_with_args::<pgrx::Uuid>(
        "select coalesce($1, gen_random_uuid());",
        vec![(PgBuiltInOids::UUIDOID.oid(), id.into_datum())],
    )
    .unwrap();

    _ = new.set_by_name("id", id);

    let ts = new.get_by_name::<pgrx::TimestampWithTimeZone>("added");

    _ = Spi::run_with_args(
        &format!("insert into {tablename}_streams values ($1) on conflict (id) do nothing;"),
        Some(vec![(PgBuiltInOids::UUIDOID.oid(), id.into_datum())]),
    );
    _ = Spi::run_with_args(
        &format!("insert into {tablename}_events values ($1, $2, 'init', jsonb_diff('{{}}'::jsonb, jsonb_strip_nulls(to_jsonb($3) - '{{id,added,updated}}'::text[])))"),
        Some(vec![
            (PgBuiltInOids::TIMESTAMPTZOID.oid(), ts.into_datum()),
            (PgBuiltInOids::UUIDOID.oid(), id.into_datum()),
            (PgBuiltInOids::RECORDOID.oid(), trigger.new().into_datum()),
        ]),
    );

    Ok(Some(new))
}

#[pg_trigger]
fn trg_upd<'a>(
    trigger: &'a pgrx::PgTrigger<'a>,
) -> Result<Option<PgHeapTuple<'a, impl WhoAllocated>>, pgrx::PgTriggerError> {
    let tablename = trigger.table_name()?;

    // TODO: handle unwrap
    let ts = Spi::get_one::<pgrx::TimestampWithTimeZone>("select clock_timestamp();").unwrap();

    let id = trigger
        .new()
        .and_then(|new| new.get_by_name::<pgrx::Uuid>("id").into_datum());

    _ = Spi::run_with_args(
        &format!(
            "insert into {tablename}_events values ($1, $2, 'set', jsonb_diff(jsonb_strip_nulls(to_jsonb($3)), jsonb_strip_nulls(to_jsonb($4))));"
        ),
        Some(vec![
            (PgBuiltInOids::TIMESTAMPTZOID.oid(), ts.into_datum()),
            (PgBuiltInOids::UUIDOID.oid(), id),
            (PgBuiltInOids::RECORDOID.oid(), trigger.old().into_datum()),
            (PgBuiltInOids::RECORDOID.oid(), trigger.new().into_datum())
        ]),
    );

    // TODO: handle unwrap
    let mut new = trigger.new().map(|new| new.into_owned()).unwrap();
    _ = new.set_by_name("updated", ts);
    Ok(Some(new))
}

#[pg_trigger]
fn trg_del<'a>(
    trigger: &'a pgrx::PgTrigger<'a>,
) -> Result<Option<PgHeapTuple<'a, impl WhoAllocated>>, pgrx::PgTriggerError> {
    let tablename = trigger.table_name()?;

    let id = trigger
        .old()
        .and_then(|old| old.get_by_name::<pgrx::Uuid>("id").into_datum());

    _ = Spi::run_with_args(
        &format!(
            r#"insert into {tablename}_events (stream_id, name, data) values ($1, 'drop', '[{{"op":"replace","path":"","value":null}}]'::jsonb);"#
        ),
        Some(vec![(PgBuiltInOids::UUIDOID.oid(), id)]),
    );

    Ok(trigger.old())
}

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn users_insert() {
        _ = Spi::run(include_str!("../sql/users.sql"));

        let user_id = Spi::get_one::<pgrx::Uuid>(
            "insert into users (username, salt, passhash) values ('foo', '', '') returning id;",
        )
        .expect("user initialized successfully");
        let (stream_id, data) =
            Spi::get_two::<pgrx::Uuid, pgrx::JsonB>("select stream_id, data from users_events;")
                .expect("user initialized successfully");

        assert_eq!(user_id, stream_id);
        assert_eq!(
            Some(serde_json::json!([
                { "op": "add", "path": "/passhash", "value": "" },
                { "op": "add", "path": "/salt", "value": "" },
                { "op": "add", "path": "/username", "value": "foo" },
            ])),
            data.map(|d| d.0)
        );
    }

    #[pg_test]
    fn users_update() {
        _ = Spi::run(include_str!("../sql/users.sql"));

        _ = Spi::run("insert into users (username, salt, passhash) values ('foo', '', '');");
        _ = Spi::run("update users set username = 'bar';");

        let name = Spi::get_one::<&str>(
            "select data -> 2 ->> 'value' from users_events where name = 'init';",
        )
        .expect("user initialized successfully");
        let data = Spi::get_one::<pgrx::JsonB>("select data from users_events where name = 'set';")
            .map(|opt| opt.map(|data| data.0));

        assert_eq!(Some("foo"), name);
        assert_eq!(
            Ok(Some(
                serde_json::json!([{"op": "replace", "path": "/username", "value": "bar"}])
            )),
            data
        );
    }

    #[pg_test]
    fn users_update_set_null() {
        _ = Spi::run(include_str!("../sql/users.sql"));

        _ = Spi::run("insert into users (username, salt, passhash, email) values ('foo', '', '', 'foo@bar.com');");
        _ = Spi::run("update users set email = null;");

        let data = Spi::get_one::<pgrx::JsonB>(
            "select data from users_events where name = 'set' order by timestamp desc;",
        )
        .map(|opt| opt.map(|data| data.0));

        assert_eq!(
            Ok(Some(
                serde_json::json!([{"op": "remove", "path": "/email"}])
            )),
            data
        );
    }

    #[pg_test]
    fn users_delete() {
        _ = Spi::run(include_str!("../sql/users.sql"));

        _ = Spi::run("insert into users (username, salt, passhash) values ('foo', '', '');");
        _ = Spi::run("delete from users;");

        let name = Spi::get_one::<&str>(
            "select data -> 2 ->> 'value' from users_events where name = 'init';",
        )
        .expect("user initialized successfully");
        let data =
            Spi::get_one::<pgrx::JsonB>("select data from users_events where name = 'drop';")
                .map(|opt| opt.map(|data| data.0));

        assert_eq!(Some("foo"), name);
        assert_eq!(
            Ok(Some(
                serde_json::json!([{"op": "replace", "path": "", "value": null}])
            )),
            data
        );
    }

    #[pg_test]
    fn users_ts() {
        _ = Spi::run(include_str!("../sql/users.sql"));

        let ts = Spi::get_one::<pgrx::TimestampWithTimeZone>("insert into users (username, salt, passhash) values ('foo', '', '') returning updated;");
        Spi::run("update users set username = 'bar' returning updated;")
            .expect("user updated successfully");

        let username = Spi::get_one_with_args::<&str>(
            "select username from users_ts($1);",
            vec![(PgBuiltInOids::TIMESTAMPTZOID.oid(), ts.into_datum())],
        );

        assert_eq!(Ok(Some("foo")), username);
    }

    #[pg_test]
    fn users_ts_commit() {
        _ = Spi::run(include_str!("../sql/users.sql"));

        let ts = Spi::get_one::<pgrx::TimestampWithTimeZone>("insert into users (username, salt, passhash) values ('foo', '', '') returning updated;");
        Spi::run("update users set username = 'bar' returning updated;")
            .expect("user updated successfully");

        _ = Spi::run_with_args(
            "select * from users_ts_commit($1);",
            Some(vec![(PgBuiltInOids::TIMESTAMPTZOID.oid(), ts.into_datum())]),
        );
        let username = Spi::get_one::<&str>("select username from users;");

        assert_eq!(Ok(Some("foo")), username);
    }

    #[pg_test]
    fn users_step() {
        _ = Spi::run(include_str!("../sql/users.sql"));

        let id = Spi::get_one::<pgrx::Uuid>(
            "insert into users (username, salt, passhash) values ('foo', '', '') returning id;",
        )
        .expect("user initialized successfully");
        _ = Spi::run("update users set username = 'bar';");
        _ = Spi::run("update users set username = 'baz';");

        let count = Spi::get_one_with_args::<i64>(
            "select count(*) from users_step($1, 1);",
            vec![(PgBuiltInOids::UUIDOID.oid(), id.into_datum())],
        );

        assert_eq!(Ok(Some(2)), count);
    }

    #[pg_test]
    fn users_snap() {
        _ = Spi::run(include_str!("../sql/users.sql"));

        _ = Spi::run("insert into users (username, salt, passhash) values ('foo', '', '');");
        _ = Spi::run("update users set username = 'bar';");
        let ts = Spi::get_one::<pgrx::TimestampWithTimeZone>(
            "update users set username = 'baz' returning updated;",
        )
        .expect("user updated successfully");

        let snap = Spi::get_one_with_args::<pgrx::JsonB>(
            "select data from users_snap($1 + interval '1 microsecond');",
            vec![(PgBuiltInOids::TIMESTAMPTZOID.oid(), ts.into_datum())],
        )
        .map(|s| s.map(|s| s.0));

        assert_eq!(
            Ok(Some(serde_json::json!([
                { "op": "add", "path": "/passhash", "value": "" },
                { "op": "add", "path": "/salt", "value": "" },
                { "op": "add", "path": "/username", "value": "baz" },
            ]))),
            snap
        );
    }

    #[pg_test]
    fn users_snap_set() {
        _ = Spi::run(include_str!("../sql/users.sql"));

        _ = Spi::run("insert into users (username, salt, passhash) values ('foo', '', '');");
        let ts = Spi::get_one::<pgrx::TimestampWithTimeZone>(
            "update users set username = 'bar' returning updated;",
        )
        .expect("user updated successfully");

        _ = Spi::run_with_args(
            "select data from users_snap_commit($1 + interval '1 microsecond');",
            Some(vec![(PgBuiltInOids::TIMESTAMPTZOID.oid(), ts.into_datum())]),
        );

        _ = Spi::run("update users set username = 'baz';");

        let snap = Spi::get_one::<pgrx::JsonB>("select data from users_events offset 2 limit 1;")
            .map(|s| s.map(|s| s.0));

        let set_event =
            Spi::get_one::<pgrx::JsonB>("select data from users_events offset 3 limit 1;")
                .map(|s| s.map(|s| s.0));

        assert_eq!(
            Ok(Some(serde_json::json!([
                { "op": "add", "path": "/passhash", "value": "" },
                { "op": "add", "path": "/salt", "value": "" },
                { "op": "add", "path": "/username", "value": "bar" },
            ]))),
            snap
        );

        assert_eq!(
            Ok(Some(serde_json::json!([
                { "op": "replace", "path": "/username", "value": "baz" },
            ]))),
            set_event
        );
    }

    #[pg_test]
    fn users_snap_events() {
        _ = Spi::run(include_str!("../sql/users.sql"));

        _ = Spi::run("insert into users (username, salt, passhash) values ('foo', '', '');");
        let ts = Spi::get_one::<pgrx::TimestampWithTimeZone>(
            "update users set username = 'bar' returning updated;",
        )
        .expect("user updated successfully");

        _ = Spi::run_with_args(
            "select data from users_snap_commit($1 + interval '1 microsecond');",
            Some(vec![(PgBuiltInOids::TIMESTAMPTZOID.oid(), ts.into_datum())]),
        );

        _ = Spi::run("update users set username = 'baz';");

        let snap = Spi::get_one::<pgrx::JsonB>("select data from users_snap_events limit 1;")
            .map(|s| s.map(|s| s.0));

        let set_event =
            Spi::get_one::<pgrx::JsonB>("select data from users_snap_events offset 1 limit 1;")
                .map(|s| s.map(|s| s.0));

        assert_eq!(
            Ok(Some(serde_json::json!([
                { "op": "add", "path": "/passhash", "value": "" },
                { "op": "add", "path": "/salt", "value": "" },
                { "op": "add", "path": "/username", "value": "bar" },
            ]))),
            snap
        );

        assert_eq!(
            Ok(Some(serde_json::json!([
                { "op": "replace", "path": "/username", "value": "baz" },
            ]))),
            set_event
        );
    }
}

/// This module is required by `cargo pgrx test` invocations.
/// It must be visible at the root of your extension crate.
#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {
        // perform one-off initialization when the pg_test framework starts
    }

    pub fn postgresql_conf_options() -> Vec<&'static str> {
        // return any postgresql.conf settings that are required for your tests
        vec![]
    }
}
