create table users (
  "id"       uuid        not null default gen_random_uuid() primary key,
  "added"    timestamptz not null default clock_timestamp(),
  "updated"  timestamptz not null default clock_timestamp(),
  "username" text        not null,
  "salt"     text        not null,
  "passhash" text        not null,
  "email"    text        null
);

create table users_streams (
  "id"       uuid not null default gen_random_uuid() primary key,
  "users_id" uuid not null
);

create unique index on users_streams ("users_id");

create table users_events (
  "timestamp" timestamptz not null default clock_timestamp(),
  "stream_id" uuid        not null references users_streams ("id") on delete cascade,
  "data"      jsonb       null,
  primary key ("stream_id", "timestamp")
);

create or replace function trg_users_ins () returns trigger language plpgsql as $$
declare stream_id uuid;
begin
  insert into users_streams (users_id) values (new.id) on conflict (users_id) do nothing returning id into stream_id;
  if stream_id is null then
    select id into stream_id from users_streams where users_id = new.id;
  end if;
  insert into users_events values
    (new.updated, stream_id, jsonb_diff('{}'::jsonb, jsonb_strip_nulls(to_jsonb(new) - '{id,added,updated}'::text[])));
  return new;
end;
$$;

create or replace function trg_users_upd () returns trigger language plpgsql as $$
declare stream_id uuid;
begin
  new.updated := clock_timestamp();
  select id into stream_id from users_streams where users_id = new.id;
  insert into users_events values
    (new.updated, stream_id, jsonb_diff(
      jsonb_strip_nulls(to_jsonb(old) - '{id,added,updated}'::text[]),
      jsonb_strip_nulls(to_jsonb(new) - '{id,added,updated}'::text[])
    ));
  return new;
end;
$$;

create or replace function trg_users_del () returns trigger language plpgsql as $$
declare stream_id uuid;
begin
  select id into stream_id from users_streams where users_id = old.id;
  insert into users_events (stream_id, data) values (stream_id, '[{"op":"replace","path":"","value":null}]'::jsonb);
  return old;
end;
$$;

create or replace trigger trg_users_ins after insert on users for each row execute function trg_users_ins();
create or replace trigger trg_users_upd before update on users for each row execute function trg_users_upd();
create or replace trigger trg_users_del before delete on users for each row execute function trg_users_del();

create or replace function users_ts (ts timestamptz default now()) returns setof users language sql as $$
  select jsonb_populate_record(
    null::users,
    jsonb_build_object(
      'id', s.users_id,
      'added', first(timestamp order by timestamp),
      'updated', last(timestamp order by timestamp)
    ) || jsonb_patch(data order by timestamp)
  )
  from users_events e
  join users_streams s on s.id = e.stream_id
  where timestamp <= ts
  group by s.users_id;
$$;

create or replace function users_step (step int default 1) returns setof users language sql as $$
  with ranked_data as (
    select
      s.users_id as id,
      timestamp,
      data,
      row_number() over (partition by s.users_id) as row_num,
      count(*) over (partition by s.users_id) as row_sum
    from users_events e
    join users_streams s on s.id = e.stream_id
  )
  select jsonb_populate_record(
    null::users,
    jsonb_build_object(
      'id', id,
      'added', first(timestamp order by timestamp),
      'updated', last(timestamp order by timestamp)
    ) || jsonb_patch(data order by timestamp)
  )
  from ranked_data
  where row_num <= row_sum - step
  group by id;
$$;

create or replace function users_commit (commit users[]) returns setof users language sql as $$
  with nest as (select * from unnest(commit))
  insert into users select id, added, clock_timestamp() as updated, username, salt, passhash, email from nest
  on conflict (id) do update set (updated, username, salt, passhash, email) =
    (select clock_timestamp() as updated, username, salt, passhash, email from nest)
  returning *;
$$;
