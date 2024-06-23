use pgrx::{pg_sys::panic::ErrorReportable, prelude::*, spi::SpiHeapTupleData};

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

#[derive(Debug)]
struct PgColumn<'a> {
    name: &'a str,
    data_type: &'a str,
}

#[derive(Debug)]
struct PgTable<'a> {
    name: &'a str,
    keys: Vec<PgColumn<'a>>,
    columns: Vec<PgColumn<'a>>,
}

impl TryFrom<SpiHeapTupleData<'_>> for PgColumn<'_> {
    type Error = spi::Error;

    fn try_from(value: SpiHeapTupleData<'_>) -> Result<Self, Self::Error> {
        if let Some((name, data_type)) = value["column_name"]
            .value()?
            .zip(value["data_type"].value()?)
        {
            Ok(Self { name, data_type })
        } else {
            Err(spi::Error::NoTupleTable)
        }
    }
}

impl<'a> PgTable<'a> {
    fn new(name: &'a str) -> Result<Self, spi::Error> {
        Ok(Self {
            name,
            keys: Spi::connect(|client| -> Result<Vec<PgColumn>, spi::Error> {
                Ok(client
                    .select(
                        include_str!("../sql/table_primary_key_columns.sql"),
                        None,
                        Some(vec![(PgBuiltInOids::TEXTOID.oid(), name.into_datum())]),
                    )?
                    .filter_map(|row| row.try_into().ok())
                    .collect())
            })?,
            columns: Spi::connect(|client| -> Result<Vec<PgColumn>, spi::Error> {
                Ok(client
                    .select(
                        include_str!("../sql/table_non_key_columns.sql"),
                        None,
                        Some(vec![(PgBuiltInOids::TEXTOID.oid(), name.into_datum())]),
                    )?
                    .filter_map(|row| row.try_into().ok())
                    .collect())
            })?,
        })
    }

    fn keys(&self) -> String {
        self.keys
            .iter()
            .map(|&PgColumn { name, data_type: _ }| name)
            .collect::<Vec<_>>()
            .join(",")
    }

    fn stream_columns(&self) -> String {
        let table = self.name;
        self.keys
            .iter()
            .map(|&PgColumn { name, data_type }| format!("{table}_{name} {data_type} not null",))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn stream_keys(&self) -> String {
        let table = self.name;
        self.keys
            .iter()
            .map(|&PgColumn { name, data_type: _ }| format!("{table}_{name}"))
            .collect::<Vec<_>>()
            .join(",")
    }

    fn trigger_values(&self) -> String {
        self.keys
            .iter()
            .map(|&PgColumn { name, data_type: _ }| format!("new.{name}"))
            .collect::<Vec<_>>()
            .join(",")
    }

    fn trigger_filter(&self) -> String {
        let table = self.name;
        self.keys
            .iter()
            .map(|&PgColumn { name, data_type: _ }| format!("{table}_{name} = new.{name}"))
            .collect::<Vec<_>>()
            .join(" and")
    }

    fn delete_trigger_filter(&self) -> String {
        let table = self.name;
        self.keys
            .iter()
            .map(|&PgColumn { name, data_type: _ }| format!("{table}_{name} = old.{name}"))
            .collect::<Vec<_>>()
            .join(" and")
    }

    fn create_tables(&self) -> Result<(), spi::Error> {
        let table = self.name;

        Spi::run(&format!(
            r#"create table {table}_streams ("id" uuid not null default gen_random_uuid() primary key, {});"#,
            self.stream_columns(),
        ))?;

        Spi::run(&format!(
            r#"create unique index on {table}_streams ({})"#,
            self.stream_keys()
        ))?;

        Spi::run(&format!(
            r#"
            create table {table}_events (
              "timestamp" timestamptz not null default clock_timestamp(),
              "stream_id" uuid        not null references {table}_streams ("id"),
              "data"      jsonb       not null,
              primary key ("stream_id", "timestamp")
            );
            "#,
        ))?;

        Spi::run(&format!(
            r#"
            create table {table}_snaps (
              "timestamp" timestamptz not null,
              "stream_id" uuid        not null references {table}_streams ("id"),
              "data"      jsonb       not null,
              primary key ("stream_id", "timestamp"),
              foreign key ("stream_id", "timestamp") references {table}_events ("stream_id", "timestamp")
            );
            "#,
        ))
    }

    fn create_triggers(&self) -> Result<(), spi::Error> {
        let table = self.name;
        let keys = self.keys();
        let stream_keys = self.stream_keys();
        let trigger_values = self.trigger_values();
        let trigger_filter = self.trigger_filter();
        let del_trg_filter = self.delete_trigger_filter();

        Spi::run(&format!(
            r#"
            create or replace function trg_{table}_ins () returns trigger language plpgsql as $$
            declare stream_id uuid;
            begin
              insert into {table}_streams ({stream_keys}) values ({trigger_values}) on conflict ({stream_keys}) do nothing returning id into stream_id;
              if stream_id is null then
                select id into stream_id from {table}_streams where {trigger_filter};
              end if;
              insert into {table}_events values
                (new.updated, stream_id, jsonb_diff('{{}}'::jsonb, jsonb_strip_nulls(to_jsonb(new) - '{{{keys},added,updated}}'::text[])));
              return new;
            end;
            $$;
            "#,
        ))?;

        Spi::run(&format!(
            r#"
            create or replace function trg_{table}_upd () returns trigger language plpgsql as $$
            declare stream_id uuid;
            begin
              new.updated := clock_timestamp();
              select id into stream_id from {table}_streams where {trigger_filter};
              insert into {table}_events values
                (new.updated, stream_id, jsonb_diff(
                  jsonb_strip_nulls(to_jsonb(old) - '{{{keys},added,updated}}'::text[]),
                  jsonb_strip_nulls(to_jsonb(new) - '{{{keys},added,updated}}'::text[])
                ));
              return new;
            end;
            $$;
            "#,
        ))?;

        Spi::run(&format!(
            r#"
            create or replace function trg_{table}_del () returns trigger language plpgsql as $$
            declare stream_id uuid;
            begin
              select id into stream_id from {table}_streams where {del_trg_filter};
              insert into {table}_events (stream_id, data) values (stream_id, '[{{"op":"replace","path":"","value":null}}]'::jsonb);
              return old;
            end;
            $$;
            "#,
        ))?;

        Spi::run(&format!(
            r#"
            create or replace trigger trg_{table}_ins after insert on {table} for each row execute function trg_{table}_ins();
            create or replace trigger trg_{table}_upd before update on {table} for each row execute function trg_{table}_upd();
            create or replace trigger trg_{table}_del before delete on {table} for each row execute function trg_{table}_del();
            "#,
        ))
    }

    fn create_functions(&self) -> Result<(), spi::Error> {
        let table = self.name;
        let keys = self.keys();
        let ts_pop = self
            .keys
            .iter()
            .map(|&PgColumn { name, data_type: _ }| format!("'{name}', s.{table}_{name}"))
            .collect::<Vec<_>>()
            .join(",");
        let group_by = self
            .keys
            .iter()
            .map(|&PgColumn { name, data_type: _ }| format!("s.{table}_{name}"))
            .collect::<Vec<_>>()
            .join(",");
        let ranked_data = self
            .keys
            .iter()
            .map(|&PgColumn { name, data_type: _ }| format!("s.{table}_{name} as {name}"))
            .collect::<Vec<_>>()
            .join(",");
        let partition_by = self
            .keys
            .iter()
            .map(|&PgColumn { name, data_type: _ }| format!("s.{table}_{name}"))
            .collect::<Vec<_>>()
            .join(",");
        let step_pop = self
            .keys
            .iter()
            .map(|&PgColumn { name, data_type: _ }| format!("'{name}', {name}"))
            .collect::<Vec<_>>()
            .join(",");
        let snaps_join = self
            .keys
            .iter()
            .map(|&PgColumn { name, data_type: _ }| format!("s.{table}_{name} = sn.{name}"))
            .collect::<Vec<_>>()
            .join(" and");
        let columns = self
            .columns
            .iter()
            .map(|&PgColumn { name, data_type: _ }| name)
            .collect::<Vec<_>>()
            .join(",");

        Spi::run(&format!(
            r#"
            create or replace function {table}_ts (ts timestamptz default now()) returns setof {table} language sql as $$
              select jsonb_populate_record(
                null::{table},
                jsonb_build_object(
                  {ts_pop},
                  'added', first(timestamp order by timestamp),
                  'updated', last(timestamp order by timestamp)
                ) || jsonb_patch(data order by timestamp)
              )
              from {table}_events e
              join {table}_streams s on s.id = e.stream_id
              where timestamp <= ts
              group by {group_by};
            $$;
            "#,
        ))?;

        Spi::run(&format!(
            r#"
            create or replace function {table}_step (step int default 1) returns setof {table} language sql as $$
              with ranked_data as (
                select
                  {ranked_data},
                  timestamp,
                  data,
                  row_number() over (partition by {partition_by}) as row_num,
                  count(*) over (partition by {partition_by}) as row_sum
                from {table}_events e
                join {table}_streams s on s.id = e.stream_id
              )
              select jsonb_populate_record(
                null::{table},
                jsonb_build_object(
                  {step_pop},
                  'added', first(timestamp order by timestamp),
                  'updated', last(timestamp order by timestamp)
                ) || jsonb_patch(data order by timestamp)
              )
              from ranked_data
              where row_num <= row_sum - step
              group by {keys};
            $$;
            "#,
        ))?;

        Spi::run(&format!(
            r#"
            create or replace function {table}_commit (commits {table}[]) returns setof {table} language sql as $$
              with nest as (select * from unnest(commits))
              insert into {table} select {keys}, added, clock_timestamp() as updated, {columns} from nest
              on conflict (id) do update set (updated, {columns}) =
                (select clock_timestamp() as updated, {columns} from nest)
              returning *;
            $$;
            "#,
        ))?;

        Spi::run(&format!(
            r#"
            create or replace function {table}_snaps_commit (snaps {table}[]) returns setof {table}_snaps language sql as $$
              insert into {table}_snaps
              select sn.updated, s.id, jsonb_diff('{{}}'::jsonb, jsonb_strip_nulls(to_jsonb(sn) - '{{{keys},added,updated}}'::text[]))
              from unnest(snaps) sn join {table}_streams s on {snaps_join} returning *;
            $$;
            "#,
        ))
    }
}

#[pg_extern]
fn init_event_source(table: &str) -> Result<(), spi::Error> {
    let primary_keys = PgTable::new(table)?;

    primary_keys.create_tables()?;
    primary_keys.create_triggers()?;
    primary_keys.create_functions()?;

    Ok(())
}

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn users_insert() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;

        let (user_id, user_ts, username) = Spi::get_three::<pgrx::Uuid, pgrx::TimestampWithTimeZone, &str>(
            "insert into users (username, salt, passhash) values ('foo', '', '') returning id, updated, username;",
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
    fn users_step() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;

        let insert =
            Spi::get_three::<pgrx::TimestampWithTimeZone, pgrx::TimestampWithTimeZone, &str>(
                "insert into users (username, salt, passhash) values ('foo', '', '') returning added, updated, username;"
            )?;
        Spi::run("update users set username = 'bar';")?;

        let ts = Spi::get_three::<pgrx::TimestampWithTimeZone, pgrx::TimestampWithTimeZone, &str>(
            "select added, updated, username from users_step();",
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
                "insert into users (username, salt, passhash) values ('foo', '', '') returning updated, username;"
            )?;
        Spi::run("update users set username = 'bar';")?;

        let updated = Spi::get_one_with_args::<pgrx::TimestampWithTimeZone>(
            "select (users_commit).updated from (select users_commit(array_agg) from (select array_agg(users_ts) from users_ts($1)));",
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
                "insert into users (username, salt, passhash) values ('foo', '', '') returning updated, username;"
            )?;
        Spi::run("delete from users;")?;

        let updated = Spi::get_one_with_args::<pgrx::TimestampWithTimeZone>(
            "select (users_commit).updated from (select users_commit(array_agg) from (select array_agg(users_ts) from users_ts($1)));",
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

    #[pg_test]
    fn users_step_commit() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;

        let username =
            Spi::get_one::<&str>(
                "insert into users (username, salt, passhash) values ('foo', '', '') returning username;"
            )?;
        Spi::run("update users set username = 'bar';")?;

        let updated = Spi::get_one::<pgrx::TimestampWithTimeZone>(
            "select (users_commit).updated from (select users_commit(array_agg) from (select array_agg(users_step) from users_step()));",
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
    fn users_snaps() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;

        let (ts, username) =
            Spi::get_two::<pgrx::TimestampWithTimeZone, &str>(
                "insert into users (username, salt, passhash) values ('foo', '', '') returning updated, username;"
            )?;
        Spi::run("update users set username = 'bar';")?;

        let (snap_ts, snap_data) = Spi::get_two_with_args::<pgrx::TimestampWithTimeZone, pgrx::JsonB>(
            "select (users_snaps_commit).timestamp, (users_snaps_commit).data from (select users_snaps_commit(array_agg) from (select array_agg(users_ts) from users_ts($1)));",
            vec![(PgBuiltInOids::TIMESTAMPTZOID.oid(), ts.into_datum())],
        )?;

        assert_eq!(
            ts, snap_ts,
            "snapped user timestamp should match snap timestamp"
        );

        assert_eq!(
            Some(serde_json::json!([
                { "op": "add", "path": "/passhash", "value": "" },
                { "op": "add", "path": "/salt", "value": "" },
                { "op": "add", "path": "/username", "value": username },
            ])),
            snap_data.map(|data| data.0),
            "snapped user data should match the json patch data in the snap"
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
