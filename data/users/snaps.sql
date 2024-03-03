create table if not exists "user_snaps" (
  "id" uuid not null default gen_random_uuid () primary key,
  "stream_id" uuid not null references "user_streams" on delete cascade,
  "timestamp" timestamptz not null default clock_timestamp(),
  "data" jsonb null
);

create unique index if not exists "idx_user_snaps_stream_id_timestamp" on "user_snaps" ("stream_id", "timestamp");

create or replace function snap_users (snap_time timestamptz) returns setof user_snaps language sql as $$
  insert into "user_snaps" (stream_id, timestamp, data)
  select stream_id, snap_time, jsonb_merge_agg (data order by timestamp)
  from "user_events"
  where timestamp < snap_time
  group by stream_id
  returning *;
$$;

create or replace function snap_user (id uuid, snap_time timestamptz) returns user_snaps language sql as $$
  insert into "user_snaps" (stream_id, timestamp, data)
  select snap_user.id, snap_time, jsonb_merge_agg (data order by timestamp)
  from "user_events"
  where stream_id = snap_user.id and timestamp < snap_time
  returning *;
$$;

create or replace view "user_latest_snaps" as
select
  last(id order by timestamp) as id,
  stream_id,
  'snap' as name,
  max(timestamp) as timestamp,
  last(data order by timestamp) as data
from "user_snaps"
group by stream_id;

create or replace view "user_latest_events" as
select e.id, e.stream_id, e.name, e.timestamp, e.data
from "user_events" e
left join "user_latest_snaps" s on s.stream_id = e.stream_id
where s is null or s.timestamp < e.timestamp;

create or replace view "user_snapped_events" as
select * from "user_latest_snaps"
union
select * from "user_latest_events";
