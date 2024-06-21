create table users (
  "id"       uuid        not null default gen_random_uuid() primary key,
  "added"    timestamptz not null default now(),
  "updated"  timestamptz not null default now(),
  "username" text        not null,
  "salt"     text        not null,
  "passhash" text        not null,
  "email"    text        null
);

create table users_streams (
  "id"       uuid not null default gen_random_uuid() primary key,
  "users_id" uuid not null
);

create unique index on users_streams ("users_id");

create table users_events (
  "timestamp" timestamptz not null default now(),
  "stream_id" uuid        not null references users_streams ("id") on delete cascade,
  "name"      text        not null,
  "data"      jsonb       null,
  primary key ("stream_id", "timestamp")
);

create or replace function trg_ins () returns trigger language plpgsql as $$
declare
  keys text;
  stream_keys text;
  aliases text;
  sql_query text;
begin
  keys := array_to_string(tg_argv, ',');
  select array_to_string(array_agg(tg_table_name || '_' || element), ',') into stream_keys from unnest(tg_argv) as t(element);
  select array_to_string(array_agg(element || ' as ' || tg_table_name || '_' || element), ',') into aliases from unnest(tg_argv) as t(element);

  sql_query := format(E'
    with streams as (
      select
        gen_random_uuid() as id,
        %s,
        jsonb_diff(\'{}\'::jsonb, jsonb_strip_nulls(to_jsonb(newtab) - \'{%s,added,updated}\'::text[])) as data
      from newtab
    ), a as (
      insert into %s_streams select id, %s from streams returning id
    )
    insert into %s_events (stream_id, name, data) select id, \'init\', data from streams;
  ', aliases, keys, tg_table_name, stream_keys, tg_table_name);

  execute sql_query;

  return null;
end;
$$;

create or replace function trg_upd () returns trigger language plpgsql as $$
declare
  keys text;
  stream_keys text;
  ts_keys text;
  sql_query text;
  ts_query text;
begin
  keys := array_to_string(tg_argv, ',');
  select array_to_string(array_agg(format('s.%s_%s = n.%s', tg_table_name, element, element)), ' and') into stream_keys from unnest(tg_argv) as t(element);
  select array_to_string(array_agg(format('%s in (select %s from newtab)', element, element)), ' and') into ts_keys from unnest(tg_argv) as t(element);

  sql_query := format(E'
    insert into %s_events (stream_id, name, data)
    select s.id, \'set\', jsonb_diff(jsonb_strip_nulls(to_jsonb(o)), jsonb_strip_nulls(to_jsonb(n)))
    from newtab n
    join oldtab o using (%s)
    join %s_streams s on %s;
  ', tg_table_name, keys, tg_table_name, stream_keys);

  execute sql_query;

  ts_query := format(E'update %s set updated = now() where %s;', tg_table_name, ts_keys);
  execute ts_query;

  return null;
end;
$$;

create or replace function trg_del () returns trigger language plpgsql as $$
declare
  stream_keys text;
  sql_query text;
begin
  select array_to_string(array_agg(format('s.%s_%s = o.%s', tg_table_name, element, element)), ' and') into stream_keys from unnest(tg_argv) as t(element);

  sql_query := format(E'
    insert into %s_events (stream_id, name, data)
    select s.id, \'drop\', \'[{"op":"replace","path":"","value":null}]\'
    from oldtab o join %s_streams s on %s;
  ', tg_table_name, tg_table_name, stream_keys);

  execute sql_query;

  return null;
end;
$$;

create or replace trigger trg_users_ins after insert on users referencing new table as newtab execute function trg_ins('id');
create or replace trigger trg_users_upd after update on users
  referencing old table as oldtab new table as newtab
  when (pg_trigger_depth() < 1)
  execute function trg_upd('id');
create or replace trigger trg_users_del after delete on users referencing old table as oldtab execute function trg_del('id');

create or replace view users_latest_snaps as
select last(timestamp) as timestamp, stream_id, 'snap' as name, last(data) as data
from users_events where name = 'snap' group by stream_id;

create or replace view users_latest_events as
select e.timestamp, e.stream_id, e.name, e.data
from users_events e
left join users_latest_snaps s on s.stream_id = e.stream_id
where s is null or s.timestamp < e.timestamp;

create or replace view users_snap_events as
select * from (select * from users_latest_snaps union select * from users_latest_events) order by timestamp;

create or replace function users_ts (ts timestamptz default now(), id uuid default null) returns setof users language sql as $$
  select jsonb_populate_record(
    null::users,
    jsonb_build_object(
      'id', s.users_id,
      'added', first(timestamp order by timestamp),
      'updated', last(timestamp order by timestamp)) || jsonb_patch(data order by timestamp)
  )
  from users_snap_events e
  join users_streams s on s.id = e.stream_id
  where timestamp <= ts and (users_ts.id is null or s.users_id = users_ts.id)
  group by s.users_id;
$$;

create or replace function users_ts_commit (ts timestamptz default now(), id uuid default null) returns setof users language sql as $$
  update users set (username, salt, passhash, email) = (
    select username, salt, passhash, email from users_ts(ts, users_ts_commit.id) where users.id = users_ts.id
  ) returning *;
$$;

create or replace function users_step (id uuid, step int) returns setof users_events language sql as $$
  with filter as (
    select e.*
    from users_snap_events e
    join users_streams s on s.id = e.stream_id
    where s.users_id = users_step.id
  )
  select * from filter limit (select count(*) - step from filter);
$$;

create or replace function users_snap (ts timestamptz default now(), id uuid default null) returns setof users_events language sql as $$
  select
    ts as timestamp,
    s.id as stream_id,
    'snap' as name,
    jsonb_diff('{}'::jsonb, jsonb_strip_nulls(to_jsonb(snap) - '{id,added,updated}'::text[])) as data
  from users_ts(ts, users_snap.id) snap
  join users_streams s on s.users_id = snap.id;
$$;

create or replace function users_snap_commit (ts timestamptz default now(), id uuid default null) returns setof users_events language sql as $$
  insert into users_events select * from users_snap(ts, users_snap_commit.id) returning *;
$$;

