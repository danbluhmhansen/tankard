create table user_streams ("id" uuid not null default gen_random_uuid() primary key);

create table user_events (
  "timestamp" timestamptz not null default clock_timestamp() primary key,
  "stream_id" uuid        not null references user_streams ("id") on delete cascade,
  "name"      text        not null,
  "data"      jsonb       null
);

create unique index "idx_user_events_stream_id_timestamp" on user_events ("stream_id", "timestamp");

create table users (
  "id"       uuid        not null primary key references user_streams ("id") on delete cascade,
  "added"    timestamptz not null default clock_timestamp(),
  "updated"  timestamptz not null default clock_timestamp(),
  "username" text        not null,
  "salt"     text        not null,
  "passhash" text        not null,
  "email"    text        null
);
