create table "user_streams" (
  "id" uuid not null default gen_random_uuid () primary key
);

create table "user_events" (
  "id" uuid not null default gen_random_uuid () primary key,
  "stream_id" uuid not null references "user_streams" on delete cascade,
  "name" text not null,
  "timestamp" timestamptz not null default clock_timestamp(),
  "data" jsonb null
);

drop index if exists "idx_user_events_stream_id";

create index "idx_user_events_stream_id" on "user_events" ("stream_id");

drop index if exists "idx_user_events_stream_id_name";

create index "idx_user_events_stream_id_name" on "user_events" ("stream_id", "name");

drop index if exists "idx_user_events_stream_id_timestamp";

create index "idx_user_events_stream_id_timestamp" on "user_events" ("stream_id", "timestamp");

