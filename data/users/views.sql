create table if not exists "users" (
  "id" uuid not null primary key references "user_streams" on delete cascade,
  "added" timestamptz not null,
  "updated" timestamptz not null,
  "username" text not null,
  "salt" text not null,
  "passhash" text not null,
  "email" text
);

create or replace function trg_user_events () returns trigger language plpgsql as $$
begin
  insert into "users"
  select stream_id, timestamp, timestamp, data ->> 'username', data ->> 'salt', data ->> 'passhash', data ->> 'email'
  from "events"
  where name = 'initialized';

  update "users"
  set
    updated = timestamp,
    username = coalesce(data ->> 'username', users.username),
    salt = coalesce(data ->> 'salt', users.salt),
    passhash = coalesce(data ->> 'passhash', users.passhash),
    email = coalesce(data ->> 'email', users.email)
  from "events"
  where events.stream_id = users.id and events.name = 'set';

  delete from "users" where id in (select stream_id from "events" where name = 'dropped');

  with aggs as (
    select
      stream_id,
      min(timestamp) as added,
      jsonb_merge_agg (data order by timestamp) as data
    from "user_snapped_events"
    where name not in ('dropped', 'restored')
    group by stream_id
  )
  insert into "users"
  select
    e.stream_id,
    a.added,
    e.timestamp,
    a.data ->> 'username',
    a.data ->> 'salt',
    a.data ->> 'passhash',
    a.data ->> 'email'
  from "events" e
  join "aggs" a using (stream_id)
  where e.name = 'restored';

  return null;
end;
$$;

create or replace trigger trg_user_events after insert on "user_events" referencing new table as "events"
for each statement execute function trg_user_events();
