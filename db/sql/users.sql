create table users (
  "id"       uuid        not null default gen_random_uuid() primary key,
  "added"    timestamptz not null default clock_timestamp(),
  "updated"  timestamptz not null default clock_timestamp(),
  "username" text        not null,
  "salt"     text        not null,
  "passhash" text        not null,
  "email"    text        null
);
