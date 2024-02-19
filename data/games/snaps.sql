create table if not exists "game_snaps" (
  "id" uuid not null default gen_random_uuid () primary key,
  "stream_id" uuid not null references "game_streams" on delete cascade,
  "timestamp" timestamptz not null default clock_timestamp(),
  "data" jsonb null
);

create unique index if not exists "idx_game_snaps_stream_id_timestamp" on "game_snaps" ("stream_id", "timestamp");

create or replace function snap_games (snap_time timestamptz) returns setof game_snaps language sql as $$
  insert into "game_snaps" (stream_id, timestamp, data)
  select stream_id, snap_time, jsonb_merge_agg (data order by timestamp)
  from "game_events"
  where timestamp < snap_time
  group by stream_id
  returning *;
$$;

create or replace function snap_game (id uuid, snap_time timestamptz) returns game_snaps language sql as $$
  insert into "game_snaps" (stream_id, timestamp, data)
  select snap_game.id, snap_time, jsonb_merge_agg (data order by timestamp)
  from "game_events"
  where stream_id = snap_game.id and timestamp < snap_time
  returning *;
$$;
