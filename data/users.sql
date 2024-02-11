drop materialized view if exists "users";

create materialized view "users" as
select
  stream_id as id,
  added,
  updated,
  data ->> 'username' as username,
  data ->> 'salt' as salt,
  data ->> 'passhash' as passhash,
  data ->> 'email' as email
from (
  select
    stream_id,
    min(timestamp) as added,
    max(timestamp) as updated,
    (array_agg(data order by timestamp desc))[1] is null as dropped,
    jsonb_merge_agg (data order by timestamp) as data
  from "user_events"
  group by stream_id
) where dropped = false;

create or replace function snap_users (snap_time timestamptz) returns setof user_snaps language sql as $$
  insert into "user_snaps" (stream_id, timestamp, data)
  select
    stream_id,
    snap_time,
    jsonb_merge_agg (data order by timestamp)
  from "user_events"
  where timestamp < snap_time
  group by stream_id
  returning *;
$$;

drop type if exists init_users_input cascade;

create type init_users_input as (username text, password text, stream_id uuid);

create
or replace function init_users (inputs init_users_input[]) returns setof user_events language sql as $$
  with nest as (
    select *, gen_salt('bf') as salt from unnest(inputs)
  ), streams as (
    insert into "user_streams" select stream_id from nest returning *
  )
  insert into "user_events" (stream_id, name, data)
  select
    stream_id,
    'initialized',
    jsonb_build_object('username', username, 'salt', salt, 'passhash', crypt(password, salt))
  from nest returning *;
$$;

drop type if exists set_usernames_input cascade;

create type set_usernames_input as (id uuid, username text);

create
or replace function set_usernames (inputs set_usernames_input[]) returns setof user_events language sql as $$
  with nest as (
    select * from unnest(inputs)
  )
  insert into "user_events" (stream_id, name, data)
  select id, 'username_set', jsonb_build_object('username', username) from nest returning *;
$$;

drop type if exists set_passwords_input cascade;

create type set_passwords_input as (id uuid, password text);

create
or replace function set_passwords (inputs set_passwords_input[]) returns setof user_events language sql as $$
  with nest as (
    select *, gen_salt('bf') as salt from unnest(inputs)
  )
  insert into "user_events" (stream_id, name, data)
  select id, 'password_set', jsonb_build_object('salt', salt, 'passhash', crypt(password, salt))
  from nest returning *;
$$;

create or replace function drop_users (inputs uuid[]) returns setof user_events language sql as $$
  insert into "user_events" (stream_id, name) values (unnest(inputs), 'dropped') returning *;
$$;

create or replace function restore_users (inputs uuid[]) returns setof user_events language sql as $$
  insert into "user_events" (stream_id, name, data) values (unnest(inputs), 'restored', '{}') returning *;
$$;
