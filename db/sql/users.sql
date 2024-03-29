create or replace function init_users (input jsonb) returns setof user_events language sql as $$
  with nest as (
    select
      coalesce((value ->> 'id')::uuid, gen_random_uuid()) as id,
      value - 'id' - 'password' as data,
      value ->> 'password' as password,
      gen_salt('bf') as salt
    from jsonb_array_elements(input)
  ), streams as (
    insert into "user_streams" select id from nest returning *
  )
  insert into "user_events" (stream_id, name, data)
  select id, 'initialized', data || jsonb_build_object('salt', salt, 'passhash', crypt(password, salt))
  from nest returning *;
$$;

create or replace function set_users (input jsonb) returns setof user_events language sql as $$
  with nest as (
    select
      (value ->> 'id')::uuid as id,
      value - 'id' - 'password' as data,
      value ->> 'password' as password,
      case when value ? 'password' then gen_salt('bf') else null end as salt 
    from jsonb_array_elements(input)
  )
  insert into "user_events" (stream_id, name, data)
  select
    id,
    'set',
    case when password is not null then
      data || jsonb_build_object('salt', salt, 'passhash', crypt(password, salt))
    else
      data
    end
  from nest returning *;
$$;

create or replace function drop_users (inputs uuid[]) returns setof user_events language sql as $$
  insert into "user_events" (stream_id, name) values (unnest(inputs), 'dropped') returning *;
$$;

create or replace function restore_users (inputs uuid[]) returns setof user_events language sql as $$
  insert into "user_events" (stream_id, name, data) values (unnest(inputs), 'restored', '{}') returning *;
$$;

create or replace function snap_users (snap_time timestamptz) returns setof user_events language sql as $$
  insert into "user_events" (stream_id, name, timestamp, data)
  select stream_id, 'snap', snap_time, jsonb_merge (data order by timestamp)
  from "user_events"
  where timestamp < snap_time
  group by stream_id
  returning *;
$$;

create or replace function snap_user (id uuid, snap_time timestamptz) returns user_events language sql as $$
  insert into "user_events" (stream_id, name, timestamp, data)
  select snap_user.id, 'snap', snap_time, jsonb_merge (data order by timestamp)
  from "user_events"
  where stream_id = snap_user.id and timestamp < snap_time
  returning *;
$$;

create or replace view "user_latest_snaps" as
select
  last(id order by timestamp) as id,
  stream_id,
  'snap' as name,
  max(timestamp) as timestamp,
  last(data order by timestamp) as data
from "user_events"
where name = 'snap'
group by stream_id;

create or replace view "user_latest_events" as
select e.id, e.stream_id, e.name, e.timestamp, e.data
from "user_events" e
left join "user_latest_snaps" s on s.stream_id = e.stream_id
where s is null or s.timestamp < e.timestamp;

create or replace view "user_snapped_events" as
select * from "user_latest_snaps"
union
select * from "user_latest_events";

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
      jsonb_merge (data order by timestamp) as data
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

create or replace function check_password (id uuid, password text) returns boolean language sql as $$
  with hash as (
    select salt, passhash from users where id = id limit 1
  )
  select crypt(password, salt) = passhash from hash limit 1;
$$;
