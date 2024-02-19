create table if not exists "user_streams" (
  "id" uuid not null default gen_random_uuid () primary key
);

create table if not exists "user_events" (
  "id" uuid not null default gen_random_uuid () primary key,
  "stream_id" uuid not null references "user_streams" on delete cascade,
  "name" text not null,
  "timestamp" timestamptz not null default clock_timestamp(),
  "data" jsonb null
);

create unique index if not exists "idx_user_events_stream_id_timestamp" on "user_events" ("stream_id", "timestamp");

