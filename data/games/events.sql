create table "game_streams" (
  "id" uuid not null default gen_random_uuid () primary key,
  "user_id" uuid not null references "user_streams" on delete cascade
);

drop index if exists "idx_game_streams_user_id" cascade;

create index "idx_game_streams_user_id" on "game_streams" ("user_id");

create table "game_events" (
  "id" uuid not null default gen_random_uuid () primary key,
  "stream_id" uuid not null references "game_streams" on delete cascade,
  "name" text not null,
  "timestamp" timestamptz not null default clock_timestamp(),
  "data" jsonb null
);

drop index if exists "idx_game_events_stream_id";

create index "idx_game_events_stream_id" on "game_events" ("stream_id");

drop index if exists "idx_game_events_stream_id_name";

create index "idx_game_events_stream_id_name" on "game_events" ("stream_id", "name");

drop index if exists "idx_game_events_stream_id_timestamp";

create index "idx_game_events_stream_id_timestamp" on "game_events" ("stream_id", "timestamp");

