drop materialized view if exists "users";

create materialized view "users" as
with snaps as (
  select id, stream_id, timestamp, data from "user_snaps"
), events as (
  select * from snaps union (
    select e.id, e.stream_id, e.timestamp, e.data
    from "user_events" e
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
  added,
  updated,
  data ->> 'username' as username,
  data ->> 'salt' as salt,
  data ->> 'passhash' as passhash,
  data ->> 'email' as email
from aggs
where dropped = false;
