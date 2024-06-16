create table users (
  "id"       uuid        not null primary key,
  "added"    timestamptz not null default clock_timestamp(),
  "updated"  timestamptz not null default clock_timestamp(),
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
  "timestamp" timestamptz not null default clock_timestamp(),
  "stream_id" uuid        not null references users_streams ("id") on delete cascade,
  "name"      text        not null,
  "data"      jsonb       null,
  primary key ("stream_id", "timestamp")
);

create or replace trigger trg_users_ins before insert on users for each row execute function trg_ins();
create or replace trigger trg_users_upd before update on users for each row execute function trg_upd();
create or replace trigger trg_users_del before delete on users for each row execute function trg_del();

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

create or replace function users_ts (ts timestamptz, id uuid default null) returns setof users language sql as $$
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

create or replace function users_ts_commit (ts timestamptz, id uuid default null) returns setof users language sql as $$
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

create or replace function users_snap (ts timestamptz, id uuid default null) returns setof users_events language sql as $$
  select
    ts as timestamp,
    s.id as stream_id,
    'snap' as name,
    jsonb_diff('{}'::jsonb, jsonb_strip_nulls(to_jsonb(snap) - '{id,added,updated}'::text[])) as data
  from users_ts(ts, users_snap.id) snap
  join users_streams s on s.users_id = snap.id;
$$;

create or replace function users_snap_commit (ts timestamptz, id uuid default null) returns setof users_events language sql as $$
  insert into users_events select * from users_snap(ts, users_snap_commit.id) returning *;
$$;

