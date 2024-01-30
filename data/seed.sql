INSERT INTO
  "stream_types" (name)
VALUES
  ('user');

INSERT INTO
  "event_streams" (id, type)
VALUES
  ('a70b2237-0cf6-4b3a-a72c-5fcccd4dd9e3', 1);

INSERT INTO
  "events" (stream_id, data)
VALUES
  (
    'a70b2237-0cf6-4b3a-a72c-5fcccd4dd9e3',
    '{"username":"foo","salt":"pepper","passhash":"password","email":"foo@bar.com"}'
  );
