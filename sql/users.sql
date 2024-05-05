create or replace trigger trg_users_ins before insert on users for each row execute function trg_ins();
create or replace trigger trg_users_upd before update on users for each row execute function trg_upd();
create or replace trigger trg_users_del before delete on users for each row execute function trg_del();

-- version control

create or replace function users_ts (ts timestamptz) returns setof users language sql as $$
  select jsonb_populate_record(
    null::users,
    jsonb_build_object('id', stream_id, 'added', first(timestamp), 'updated', last(timestamp)) || jsonb_patch(data)
  )
  from user_events where timestamp < ts group by stream_id;
$$;

create or replace function users_ts_commit (ts timestamptz) returns setof users language sql as $$
  update users set (username, salt, passhash, email) = (
    select username, salt, passhash, email from users_ts(ts) where users.id = users_ts.id
  ) returning *;
$$;

create or replace function users_step (id uuid, step int) returns setof user_events language sql as $$
  with filter as (
    select * from user_events where stream_id = id
  )
  select * from filter limit (select count(*) - step from filter);
$$;
