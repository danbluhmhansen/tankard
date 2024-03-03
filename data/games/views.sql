create table if not exists "games" (
  "id" uuid not null primary key references "game_streams" on delete cascade,
  "user_id" uuid not null references users on delete cascade,
  "added" timestamptz not null,
  "updated" timestamptz not null,
  "name" text not null,
  "description" text
);

create index if not exists "idx_games_user_id" on "games" ("user_id");

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
      jsonb_merge_agg (data order by timestamp) as data
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
