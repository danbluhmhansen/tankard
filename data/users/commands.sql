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
