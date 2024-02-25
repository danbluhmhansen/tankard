create or replace function init_games (input jsonb) returns setof game_events language sql as $$
  with nest as (
    select
      coalesce((value ->> 'id')::uuid, gen_random_uuid()) as id,
      (value ->> 'user_id')::uuid as user_id,
      value - 'id' - 'user_id' as data
    from jsonb_array_elements(input)
  ), streams as (
    insert into "game_streams" select id, user_id from nest returning *
  )
  insert into "game_events" (stream_id, name, data) select id, 'initialized', data from nest returning *;
$$;

create or replace function set_games (input jsonb) returns setof game_events language sql as $$
  with nest as (
    select (value ->> 'id')::uuid as id, value - 'id' as data from jsonb_array_elements(input)
  )
  insert into "game_events" (stream_id, name, data) select id, 'set', data from nest returning *;
$$;

create or replace function drop_games (inputs uuid[]) returns setof game_events language sql as $$
  insert into "game_events" (stream_id, name) values (unnest(inputs), 'dropped') returning *;
$$;

create or replace function restore_games (inputs uuid[]) returns setof game_events language sql as $$
  insert into "game_events" (stream_id, name, data) values (unnest(inputs), 'restored', '{}') returning *;
$$;
