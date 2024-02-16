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
