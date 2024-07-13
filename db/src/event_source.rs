use pgrx::{prelude::*, spi::SpiHeapTupleData};

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
            .join(" and ")
    }

    fn delete_trigger_filter(&self) -> String {
        let table = self.name;
        self.keys
            .iter()
            .map(|&PgColumn { name, data_type: _ }| format!("{table}_{name} = old.{name}"))
            .collect::<Vec<_>>()
            .join(" and ")
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
              insert into {table}_events (stream_id, data) values (stream_id, '[{{"op":"replace","path":"","value":{{}}}}]'::jsonb);
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
            .join(" and ");
        let columns = self
            .columns
            .iter()
            .map(|&PgColumn { name, data_type: _ }| name)
            .collect::<Vec<_>>()
            .join(",");
        let stream_keys = self
            .keys
            .iter()
            .map(|&PgColumn { name, data_type: _ }| format!("{table}_{name}"))
            .collect::<Vec<_>>()
            .join(",");
        let del_streams = self
            .keys
            .iter()
            .map(|&PgColumn { name, data_type: _ }| {
                format!("{name} in (select {table}_{name} from streams)")
            })
            .collect::<Vec<_>>()
            .join(" and ");

        Spi::run(&format!(
            r#"
            create or replace function {table}_ts (ts timestamptz default now()) returns setof {table} language sql
            as $$
                select
                    jsonb_populate_record(
                        null::{table},
                        jsonb_build_object(
                            {ts_pop},
                            'added', e.added,
                            'updated', e.updated
                        ) || jsonb_patch('{{}}', e.data)
                    )
                from (
                    select
                        stream_id,
                        first(timestamp order by timestamp) as added,
                        last(timestamp order by timestamp) as updated,
                        jsonb_agg(data order by timestamp) as data
                    from (
                        select timestamp, stream_id, jsonb_array_elements(data) as data
                        from {table}_events
                        where timestamp <= ts
                    )
                    group by stream_id
                    having last(data order by timestamp) <> '{{"op": "replace", "path": "", "value": {{}}}}'
                ) e
                join {table}_streams s on s.id = e.stream_id;
            $$;
            "#,
        ))?;

        Spi::run(&format!(
            r#"
            create or replace function {table}_ts_del (ts timestamptz default now()) returns table (stream_id uuid)
            language sql as $$
                select stream_id
                from (
                    select timestamp, stream_id, jsonb_array_elements(data) as data
                    from {table}_events
                    where timestamp <= ts
                )
                group by stream_id
                having last(data order by timestamp) = '{{"op": "replace", "path": "", "value": {{}}}}';
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
                ) || jsonb_patch_agg(data order by timestamp)
              )
              from ranked_data
              where row_num <= row_sum - step
              group by {keys};
            $$;
            "#,
        ))?;

        Spi::run(&format!(
            r#"
            create or replace function {table}_commit (
                commits {table}[] default null,
                stream_ids uuid[] default null
            ) returns setof {table} language sql as $$
                with nest as (select * from unnest(commits)),
                streams as (
                    select {stream_keys} from {table}_streams where id in (select unnest(stream_ids))
                ),
                ins as (
                    insert into {table} select {keys}, added, clock_timestamp() as updated, {columns} from nest
                    on conflict ({keys}) do update set (updated, {columns}) =
                    (select clock_timestamp() as updated, {columns} from nest)
                    returning *
                ),
                del as (
                    delete from {table} where {del_streams} returning *
                )
                select * from ins union select * from del;
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
    primary_keys.create_functions()
}

#[pg_extern]
fn refresh_event_triggers(table: &str) -> Result<(), spi::Error> {
    PgTable::new(table)?.create_triggers()
}

#[pg_extern]
fn refresh_event_functions(table: &str) -> Result<(), spi::Error> {
    PgTable::new(table)?.create_functions()
}

#[pg_extern]
fn drop_event_source(table: &str) -> Result<(), spi::Error> {
    Spi::run(&format!("drop function if exists {table}_snaps_commit({table}[]), {table}_commit({table}[]), {table}_step(int), {table}_ts(timestamptz), trg_{table}_del(), trg_{table}_upd(), trg_{table}_ins() cascade;"))
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn users_drop_event_source() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;
        Spi::run("select drop_event_source('users');")
    }

    #[pg_test]
    fn composite_keys() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/composite_keys.sql"))
    }
}
