create table if not exists "game_streams" (
  "id" uuid not null default gen_random_uuid () primary key,
  "user_id" uuid not null references "user_streams" on delete cascade
);

create index if not exists "idx_game_streams_user_id" on "game_streams" ("user_id");

create table if not exists "game_events" (
  "id" uuid not null default gen_random_uuid () primary key,
  "stream_id" uuid not null references "game_streams" on delete cascade,
  "name" text not null,
  "timestamp" timestamptz not null default clock_timestamp(),
  "data" jsonb null
);

create unique index if not exists "idx_game_events_stream_id_timestamp" on "game_events" ("stream_id", "timestamp");

