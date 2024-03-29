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
create or replace function snap_games (snap_time timestamptz) returns setof game_events language sql as $$
  insert into "game_events" (stream_id, name, timestamp, data)
  select stream_id, 'snap', snap_time, jsonb_merge (data order by timestamp)
  from "game_events"
  where timestamp < snap_time
  group by stream_id
  returning *;
$$;

create or replace function snap_games (snap_time timestamptz) returns setof game_events language sql as $$
  insert into "game_events" (stream_id, timestamp, data)
  select stream_id, snap_time, jsonb_merge (data order by timestamp)
  from "game_events"
  where timestamp < snap_time
  group by stream_id
  returning *;
$$;

create or replace function snap_game (id uuid, snap_time timestamptz) returns game_events language sql as $$
  insert into "game_events" (stream_id, timestamp, data)
  select snap_game.id, snap_time, jsonb_merge (data order by timestamp)
  from "game_events"
  where stream_id = snap_game.id and timestamp < snap_time
  returning *;
$$;

create or replace view "game_latest_snaps" as
select
  last(id order by timestamp) as id,
  stream_id,
  'snap' as name,
  max(timestamp) as timestamp,
  last(data order by timestamp) as data
from "game_events"
group by stream_id;

create or replace view "game_latest_events" as
select e.id, e.stream_id, e.name, e.timestamp, e.data
from "game_events" e
left join "game_latest_snaps" s on s.stream_id = e.stream_id
where s is null or s.timestamp < e.timestamp;

create or replace view "game_snapped_events" as
select * from "game_latest_snaps"
union
select * from "game_latest_events";

create or replace function trg_game_events () returns trigger language plpgsql as $$
begin
  insert into "games"
  select stream_id, user_id, timestamp, timestamp, data ->> 'name', data ->> 'description'
  from "events" e
  join "game_streams" s on s.id = e.stream_id
  where name = 'initialized';

  update "games"
  set
    updated = timestamp,
    name = coalesce(data ->> 'name', games.name),
    description = coalesce(data ->> 'description', games.description)
  from "events"
  where events.stream_id = games.id and events.name = 'set';

  delete from "games" where id in (select stream_id from "events" where name = 'dropped');

  with aggs as (
    select
      stream_id,
      min(timestamp) as added,
      jsonb_merge (data order by timestamp) as data
    from "game_snapped_events"
    where name not in ('dropped', 'restored')
    group by stream_id
  )
  insert into "games"
  select
    e.stream_id,
    s.user_id,
    a.added,
    e.timestamp,
    a.data ->> 'name',
    a.data ->> 'description'
  from "events" e
  join "game_streams" s on s.id = e.stream_id
  join "aggs" a using (stream_id)
  where e.name = 'restored';

  return null;
end;
$$;

create or replace trigger trg_game_events after insert on "game_events" referencing new table as "events"
for each statement execute function trg_game_events();
