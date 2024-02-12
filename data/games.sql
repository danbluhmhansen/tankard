drop materialized view if exists "games";

create materialized view "games" as
with snaps as (
  select id, stream_id, timestamp, data from "game_snaps"
), events as (
  select * from snaps union (
    select e.id, e.stream_id, e.timestamp, e.data
    from "game_events" e
    where not exists (select 1 from snaps s where s.stream_id = e.stream_id and s.timestamp > e.timestamp)
  )
  order by stream_id, timestamp
), aggs as (
  select
    stream_id,
    min(timestamp) as added,
    max(timestamp) as updated,
    (array_agg(data order by timestamp desc))[1] is null as dropped,
    jsonb_merge_agg (data order by timestamp) as data
    from events
    group by stream_id
)
select
  stream_id as id,
  user_id,
  added,
  updated,
  data ->> 'name' as name,
  data ->> 'description' as description
from aggs
join "game_streams" s on s.id = stream_id
where dropped = false;

create or replace function snap_games (snap_time timestamptz) returns setof game_snaps language sql as $$
  insert into "game_snaps" (stream_id, timestamp, data)
  select stream_id, snap_time, jsonb_merge_agg (data order by timestamp)
  from "game_events"
  where timestamp < snap_time
  group by stream_id
  returning *;
$$;

create or replace function snap_game (id uuid, snap_time timestamptz) returns game_snaps language sql as $$
  insert into "game_snaps" (stream_id, timestamp, data)
  select snap_game.id, snap_time, jsonb_merge_agg (data order by timestamp)
  from "game_events"
  where stream_id = snap_game.id and timestamp < snap_time
  returning *;
$$;

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
  insert into "game_events" (stream_id, name) values (unnest(inputs), 'dropped') returning *;
$$;

create or replace function restore_games (inputs uuid[]) returns setof game_events language sql as $$
  insert into "game_events" (stream_id, name, data) values (unnest(inputs), 'restored', '{}') returning *;
$$;
