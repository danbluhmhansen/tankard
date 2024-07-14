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
    added: Option<&'a str>,
    updated: Option<&'a str>,
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
            added: None,
            updated: None,
        })
    }

    fn added(mut self, added: &'a str) -> Self {
        self.added = Some(added);
        self
    }

    fn updated(mut self, updated: &'a str) -> Self {
        self.updated = Some(updated);
        self
    }

    fn keys(&self) -> String {
        self.keys
            .iter()
            .map(|&PgColumn { name, data_type: _ }| name)
            .collect::<Vec<_>>()
            .join(",")
    }

    fn json_strip_cols(&self) -> String {
        self.keys
            .iter()
            .map(|&PgColumn { name, data_type: _ }| name)
            .chain(std::iter::once(self.added).flatten())
            .chain(std::iter::once(self.updated).flatten())
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
        ))
    }

    fn create_triggers(&self) -> Result<(), spi::Error> {
        let table = self.name;
        let json_strip_cols = self.json_strip_cols();
        let stream_keys = self.stream_keys();
        let trigger_values = self.trigger_values();
        let trigger_filter = self.trigger_filter();
        let del_trg_filter = self.delete_trigger_filter();

        Spi::run(&format!(
            r#"
            create or replace function trg_{table}_ins () returns trigger language plpgsql as $$
            declare stream_id uuid;
            begin
                insert into {table}_streams ({stream_keys}) values ({trigger_values})
                    on conflict ({stream_keys}) do nothing returning id into stream_id;
                if stream_id is null then
                    select id into stream_id from {table}_streams where {trigger_filter};
                end if;
                insert into {table}_events values (
                    new.updated,
                    stream_id,
                    jsonb_diff('{{}}'::jsonb, jsonb_strip_nulls(to_jsonb(new) - '{{{json_strip_cols}}}'::text[]))
                );
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
                insert into {table}_events values (new.updated, stream_id, jsonb_diff(
                    jsonb_strip_nulls(to_jsonb(old) - '{{{json_strip_cols}}}'::text[]),
                    jsonb_strip_nulls(to_jsonb(new) - '{{{json_strip_cols}}}'::text[])
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
                insert into {table}_events (stream_id, data) values
                    (stream_id, '[{{"op":"replace","path":"","value":{{}}}}]'::jsonb);
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
            .chain(
                std::iter::once(self.added)
                    .flatten()
                    .map(|a| format!("'{a}', e.{a}")),
            )
            .chain(
                std::iter::once(self.updated)
                    .flatten()
                    .map(|u| format!("'{u}', e.{u}")),
            )
            .collect::<Vec<_>>()
            .join(",");
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
        let added = if let Some(added) = self.added {
            format!("first(timestamp order by timestamp) as {added},")
        } else {
            "".to_string()
        };
        let updated = if let Some(updated) = self.updated {
            format!("last(timestamp order by timestamp) as {updated},")
        } else {
            "".to_string()
        };

        Spi::run(&format!(
            r#"
            create or replace function {table}_ts (ts timestamptz default now()) returns setof {table} language sql
            as $$
                select
                    jsonb_populate_record(
                        null::{table},
                        jsonb_build_object({ts_pop}) || jsonb_patch('{{}}', e.data)
                    )
                from (
                    select stream_id,{added}{updated}jsonb_agg(data order by timestamp) as data
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
                select
                    jsonb_populate_record(
                        null::{table},
                        jsonb_build_object({ts_pop}) || jsonb_patch('{{}}', e.data)
                    )
                from (
                    select stream_id,{added}{updated}jsonb_agg(data order by timestamp) as data
                    from (
                        select timestamp, stream_id, jsonb_array_elements(data) as data
                        from (
                            select
                                stream_id,
                                timestamp,
                                data,
                                row_number() over (partition by stream_id) as row_num,
                                count(*) over (partition by stream_id) as row_sum
                            from {table}_events
                        )
                        where row_num <= row_sum - step
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
            create or replace function {table}_step_del (step int default 1) returns table (stream_id uuid) language sql
            as $$
                select stream_id
                from (
                    select timestamp, stream_id, jsonb_array_elements(data) as data
                    from (
                        select
                            stream_id,
                            timestamp,
                            data,
                            row_number() over (partition by stream_id) as row_num,
                            count(*) over (partition by stream_id) as row_sum
                        from {table}_events
                    )
                    where row_num <= row_sum - step
                )
                group by stream_id
                having last(data order by timestamp) = '{{"op": "replace", "path": "", "value": {{}}}}'
            $$;
            "#,
        ))?;

        let commit_added = if let Some(added) = self.added {
            format!("{added},")
        } else {
            "".to_string()
        };
        let commit_updated = if let Some(updated) = self.updated {
            format!("clock_timestamp() as {updated},")
        } else {
            "".to_string()
        };
        let conflict_updated = if let Some(updated) = self.updated {
            format!("{updated},")
        } else {
            "".to_string()
        };
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
                    insert into {table} select {keys}, {commit_added} {commit_updated} {columns} from nest
                    on conflict ({keys}) do update set ({conflict_updated} {columns}) =
                    (select {commit_updated} {columns} from nest)
                    returning *
                ),
                del as (
                    delete from {table} where {del_streams} returning *
                )
                select * from ins union select * from del;
            $$;
            "#,
        ))
    }
}

#[pg_extern]
fn init_event_source(
    table: &str,
    added: default!(&str, "''"),
    updated: default!(&str, "''"),
) -> Result<(), spi::Error> {
    let mut table = PgTable::new(table)?;

    if !added.is_empty() {
        table = table.added(added);
    }
    if !updated.is_empty() {
        table = table.updated(updated);
    }

    table.create_tables()?;
    table.create_triggers()?;
    table.create_functions()
}

#[pg_extern]
fn refresh_event_triggers(
    table: &str,
    added: default!(&str, "''"),
    updated: default!(&str, "''"),
) -> Result<(), spi::Error> {
    let mut table = PgTable::new(table)?;

    if !added.is_empty() {
        table = table.added(added);
    }
    if !updated.is_empty() {
        table = table.updated(updated);
    }

    table.create_triggers()
}

#[pg_extern]
fn refresh_event_functions(
    table: &str,
    added: default!(&str, "''"),
    updated: default!(&str, "''"),
) -> Result<(), spi::Error> {
    let mut table = PgTable::new(table)?;

    if !added.is_empty() {
        table = table.added(added);
    }
    if !updated.is_empty() {
        table = table.updated(updated);
    }

    table.create_functions()
}

#[pg_extern]
fn drop_event_source(table: &str) -> Result<(), spi::Error> {
    Spi::run(&format!("drop function if exists {table}_commit({table}[]), {table}_step(int), {table}_ts(timestamptz), trg_{table}_del(), trg_{table}_upd(), trg_{table}_ins() cascade;"))
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn users_init_event_source() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;
        Spi::run("select init_event_source('users', 'added', 'updated');")
    }

    #[pg_test]
    fn users_drop_event_source() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;
        Spi::run("select init_event_source('users', 'added', 'updated');")?;
        Spi::run("select drop_event_source('users');")
    }

    #[pg_test]
    fn composite_keys() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/composite_keys.sql"))
    }

    #[pg_test]
    fn no_ts_cols() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/no_ts_cols.sql"))
    }
}
