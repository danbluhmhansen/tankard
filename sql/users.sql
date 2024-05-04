-- track user inserts trigger

create or replace function trg_users_ins () returns trigger language plpgsql as $$
begin
  new.id := coalesce(new.id, gen_random_uuid());
  insert into user_streams values (new.id) on conflict (id) do nothing;
  insert into user_events (stream_id, name, data) values
    (new.id, 'init', jsonb_strip_nulls(to_jsonb(new) - '{id,added,updated}'::text[]));
  return new;
end
$$;

create or replace trigger trg_users_ins before insert on users for each row execute function trg_users_ins();

-- track user changes trigger

create or replace function trg_users_upd () returns trigger language plpgsql as $$
begin
  insert into user_events (stream_id, name, data) values
    (new.id, 'set', jsonb_diff(jsonb_strip_nulls(to_jsonb(old)), jsonb_strip_nulls(to_jsonb(new))));
  new.updated := clock_timestamp();
  return new;
end
$$;

create or replace trigger trg_users_upd before update on users for each row execute function trg_users_upd();

-- track user drops trigger

create or replace function trg_users_del () returns trigger language plpgsql as $$
begin
  insert into user_events (stream_id, name, data) values
    (old.id, 'drop', '[{"op":"replace","path":"","value":null}]'::jsonb);
  return old;
end
$$;

create or replace trigger trg_users_del before delete on users for each row execute function trg_users_del();
