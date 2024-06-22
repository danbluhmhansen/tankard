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

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn users_insert() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;

        let (user_id, user_ts, username) = Spi::get_three::<pgrx::Uuid, pgrx::TimestampWithTimeZone, &str>(
            "insert into users (username, salt, passhash) values ('foo', '', '') returning id, added, username;",
        )?;

        let (stream_id, stream_user_id) =
            Spi::get_two::<pgrx::Uuid, pgrx::Uuid>("select id, users_id from users_streams;")?;

        assert_eq!(
            user_id, stream_user_id,
            "generated user id should match the stream id"
        );

        let (event_ts, event_stream_id, event_data) =
            Spi::get_three::<pgrx::TimestampWithTimeZone, pgrx::Uuid, pgrx::JsonB>(
                "select timestamp, stream_id, data from users_events;",
            )?;

        assert_eq!(
            user_ts, event_ts,
            "added user timestamp should match event timestamp"
        );

        assert_eq!(
            stream_id, event_stream_id,
            "stream id should match the event's stream id"
        );

        assert_eq!(
            Some(serde_json::json!([
                { "op": "add", "path": "/passhash", "value": "" },
                { "op": "add", "path": "/salt", "value": "" },
                { "op": "add", "path": "/username", "value": username },
            ])),
            event_data.map(|data| data.0),
            "added user data should match the json patch data in the event"
        );

        Ok(())
    }

    #[pg_test]
    fn users_update() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;

        Spi::run("insert into users (username, salt, passhash) values ('foo', '', '');")?;
        let (user_ts, username) = Spi::get_two::<pgrx::TimestampWithTimeZone, &str>(
            "update users set username = 'bar' returning updated, username;",
        )?;

        let (event_ts, event_data) = Spi::get_two::<pgrx::TimestampWithTimeZone, pgrx::JsonB>(
            "select timestamp, data from users_events offset 1;",
        )?;

        assert_eq!(
            user_ts, event_ts,
            "updated user timestamp should match event timestamp"
        );

        assert_eq!(
            Some(serde_json::json!([
                { "op": "replace", "path": "/username", "value": username },
            ])),
            event_data.map(|data| data.0),
            "updated user data should match the json patch data in the event"
        );

        Ok(())
    }

    #[pg_test]
    fn users_update_set_null() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;

        Spi::run("insert into users (username, salt, passhash, email) values ('foo', '', '', 'foo@bar.com');")?;
        let user_ts = Spi::get_one::<pgrx::TimestampWithTimeZone>(
            "update users set email = null returning updated;",
        )?;

        let (event_ts, event_data) = Spi::get_two::<pgrx::TimestampWithTimeZone, pgrx::JsonB>(
            "select timestamp, data from users_events offset 1;",
        )?;

        assert_eq!(
            user_ts, event_ts,
            "updated user timestamp should match event timestamp"
        );

        assert_eq!(
            Some(serde_json::json!([
                { "op": "remove", "path": "/email" },
            ])),
            event_data.map(|data| data.0),
            "updated user data should match the json patch data in the event"
        );

        Ok(())
    }

    #[pg_test]
    fn users_delete() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;

        Spi::run("insert into users (username, salt, passhash) values ('foo', '', '');")?;
        Spi::run("delete from users;")?;

        let event_data = Spi::get_one::<pgrx::JsonB>("select data from users_events offset 1;")?;

        assert_eq!(
            Some(serde_json::json!([{"op": "replace", "path": "", "value": null}])),
            event_data.map(|data| data.0),
            "delete event should indicate the user has been deleted"
        );

        Ok(())
    }

    #[pg_test]
    fn users_ts() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;

        let insert =
            Spi::get_three::<pgrx::TimestampWithTimeZone, pgrx::TimestampWithTimeZone, &str>(
                "insert into users (username, salt, passhash) values ('foo', '', '') returning added, updated, username;"
            )?;
        Spi::run("update users set username = 'bar';")?;

        let ts = Spi::get_three_with_args::<
            pgrx::TimestampWithTimeZone,
            pgrx::TimestampWithTimeZone,
            &str,
        >(
            "select added, updated, username from users_ts($1);",
            vec![(PgBuiltInOids::TIMESTAMPTZOID.oid(), insert.0.into_datum())],
        )?;

        assert_eq!(
            insert, ts,
            "user fetched from insert timestamp should match originally inserted user"
        );

        Ok(())
    }

    #[pg_test]
    fn users_ts_deleted() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;

        let insert =
            Spi::get_three::<pgrx::TimestampWithTimeZone, pgrx::TimestampWithTimeZone, &str>(
                "insert into users (username, salt, passhash) values ('foo', '', '') returning added, updated, username;"
            )?;
        Spi::run("delete from users;")?;

        let ts = Spi::get_three_with_args::<
            pgrx::TimestampWithTimeZone,
            pgrx::TimestampWithTimeZone,
            &str,
        >(
            "select added, updated, username from users_ts($1);",
            vec![(PgBuiltInOids::TIMESTAMPTZOID.oid(), insert.0.into_datum())],
        )?;

        assert_eq!(
            insert, ts,
            "user fetched from insert timestamp should match originally inserted user"
        );

        Ok(())
    }

    #[pg_test]
    fn users_ts_commit() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;

        let (ts, username) =
            Spi::get_two::<pgrx::TimestampWithTimeZone, &str>(
                "insert into users (username, salt, passhash) values ('foo', '', '') returning added, username;"
            )?;
        Spi::run("update users set username = 'bar';")?;

        let updated = Spi::get_one_with_args::<pgrx::TimestampWithTimeZone>(
            "select updated from users_ts_commit($1);",
            vec![(PgBuiltInOids::TIMESTAMPTZOID.oid(), ts.into_datum())],
        )?;

        let (event_ts, event_data) = Spi::get_two::<pgrx::TimestampWithTimeZone, pgrx::JsonB>(
            "select timestamp, data from users_events offset 2;",
        )?;

        assert_eq!(
            updated, event_ts,
            "commited user timestamp should match event timestamp"
        );

        assert_eq!(
            Some(serde_json::json!([
                { "op": "replace", "path": "/username", "value": username },
            ])),
            event_data.map(|data| data.0),
            "commited user data should match the json patch data in the event"
        );

        Ok(())
    }

    #[pg_test]
    fn users_ts_commit_deleted() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;

        let (ts, username) =
            Spi::get_two::<pgrx::TimestampWithTimeZone, &str>(
                "insert into users (username, salt, passhash) values ('foo', '', '') returning added, username;"
            )?;
        Spi::run("delete from users;")?;

        let updated = Spi::get_one_with_args::<pgrx::TimestampWithTimeZone>(
            "select updated from users_ts_commit($1);",
            vec![(PgBuiltInOids::TIMESTAMPTZOID.oid(), ts.into_datum())],
        )?;

        let (event_ts, event_data) = Spi::get_two::<pgrx::TimestampWithTimeZone, pgrx::JsonB>(
            "select timestamp, data from users_events offset 2;",
        )?;

        assert_eq!(
            updated, event_ts,
            "commited user timestamp should match event timestamp"
        );

        assert_eq!(
            Some(serde_json::json!([
                { "op": "add", "path": "/passhash", "value": "" },
                { "op": "add", "path": "/salt", "value": "" },
                { "op": "add", "path": "/username", "value": username },
            ])),
            event_data.map(|data| data.0),
            "commited user data should match the json patch data in the event"
        );

        Ok(())
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
