create table test_table (
  "id1"     uuid        not null default gen_random_uuid(),
  "id2"     uuid        not null default gen_random_uuid(),
  "added"   timestamptz not null default clock_timestamp(),
  "updated" timestamptz not null default clock_timestamp(),
  "name"    text        not null,
  primary key (id1, id2)
);

select init_event_source('test_table', 'added', 'updated');
