drop materialized view if exists "users";

create materialized view "users" as
select
  s.id,
  e.added,
  e.updated,
  e.data ->> 'username' as username,
  e.data ->> 'salt' as salt,
  e.data ->> 'passhash' as passhash,
  e.data ->> 'email' as email
from
  "user_streams" s
  join (
    select
      stream_id,
      min(timestamp) as added,
      max(timestamp) as updated,
      jsonb_merge_agg (data) as data
    from
      "user_events"
    group by
      stream_id
  ) e on e.stream_id = s.id;

drop type if exists init_user_input cascade;

create type init_user_input as (username text, password text, stream_id uuid);

create
or replace function init_user (inputs init_user_input[]) returns setof user_events language sql as $$
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

drop type if exists set_username_input cascade;

create type set_username_input as (stream_id uuid, username text);

create
or replace function set_username (inputs set_username_input[]) returns setof user_events language sql as $$
  with nest as (
    select * from unnest(inputs)
  )
  insert into "user_events" (stream_id, name, data)
  select stream_id, 'username_set', jsonb_build_object('username', username) from nest returning *;
$$;

drop type if exists set_password_input cascade;

create type set_password_input as (stream_id uuid, password text);

create
or replace function set_password (inputs set_password_input[]) returns setof user_events language sql as $$
  with nest as (
    select *, gen_salt('bf') as salt from unnest(inputs)
  )
  insert into "user_events" (stream_id, name, data)
  select stream_id, 'password_set', jsonb_build_object('salt', salt, 'passhash', crypt(password, salt))
  from nest returning *;
$$;
