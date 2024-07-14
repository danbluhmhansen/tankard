create table test_table (
  "id"   uuid not null default gen_random_uuid() primary key,
  "name" text not null
);

select init_event_source('test_table');
