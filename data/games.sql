drop materialized view if exists "games";

create materialized view "games" as
select
  s.id,
  s.user_id,
  e.added,
  e.updated,
  e.data ->> 'name' as name,
  e.data ->> 'description' as description
from
  "game_streams" s
  join (
    select
      stream_id,
      min(timestamp) as added,
      max(timestamp) as updated,
      jsonb_merge_agg (data) as data
    from "game_events"
    group by stream_id
  ) e on e.stream_id = s.id;

drop type if exists init_games_input cascade;

create type init_games_input as (
  user_id uuid,
  name text,
  description text,
  stream_id uuid
);

create
or replace function init_games (inputs init_games_input[]) returns setof game_events language sql as $$
  with nest as (
    select * from unnest(inputs)
  ), streams as (
    insert into "game_streams" select stream_id, user_id from nest returning *
  )
  insert into "game_events" (stream_id, name, data)
  select stream_id, 'initialized', jsonb_build_object('name', name, 'description', description)
  from nest returning *;
$$;

drop type if exists set_game_names_input cascade;

create type set_game_names_input as (
  id uuid,
  name text
);

create or replace function set_game_names (inputs set_game_names_input[]) returns setof game_events language sql as $$
  with nest as (
    select * from unnest(inputs)
  )
  insert into "game_events" (stream_id, name, data)
  select id, 'name_set', jsonb_build_object('name', name) from nest returning *;
$$;

drop type if exists set_game_descriptions_input cascade;

create type set_game_descriptions_input as (
  id uuid,
  description text
);

create or replace function set_game_descriptions (inputs set_game_descriptions_input[])
returns setof game_events language sql as $$
  with nest as (
    select * from unnest(inputs)
  )
  insert into "game_events" (stream_id, name, data)
  select id, 'description_set', jsonb_build_object('description', description) from nest returning *;
$$;

create or replace function drop_games (inputs uuid[]) returns setof game_events language sql as $$
  insert into "game_events" (stream_id, name) values(unnest(inputs), 'dropped') returning *;
$$;
