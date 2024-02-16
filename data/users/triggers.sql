create or replace function notify_user_events_inserted() returns trigger language plpgsql as $$
begin
  perform pg_notify('tankard', 'user_events_inserted');
  return new;
end;
$$;

create or replace trigger "trg_user_events_inserted" after insert on "user_events"
execute function notify_user_events_inserted();
