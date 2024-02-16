create table "user_snaps" (
  "id" uuid not null default gen_random_uuid () primary key,
  "stream_id" uuid not null references "user_streams" on delete cascade,
  "timestamp" timestamptz not null default clock_timestamp(),
  "data" jsonb null
);

drop index if exists "idx_user_snaps_stream_id";

create index "idx_user_snaps_stream_id" on "user_snaps" ("stream_id");

drop index if exists "idx_user_snaps_stream_id_timestamp";

create index "idx_user_snaps_stream_id_timestamp" on "user_snaps" ("stream_id", "timestamp");

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
