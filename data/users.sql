DROP MATERIALIZED VIEW IF EXISTS "users";
CREATE MATERIALIZED VIEW "users" AS
  SELECT
    s.id,
    e.added,
	e.updated,
    e.data ->> 'username' AS username,
    e.data ->> 'salt' AS salt,
    e.data ->> 'passhash' AS passhash,
    e.data ->> 'email' AS email
  FROM
    "user_streams" s
    JOIN (
      SELECT
        stream_id,
        min(timestamp) AS added,
        max(timestamp) AS updated,
        jsonb_merge_agg(data) AS data
      FROM
        "user_events"
      GROUP BY
        stream_id
    ) e ON e.stream_id = s.id;

DROP TYPE IF EXISTS init_user_input CASCADE;
CREATE TYPE init_user_input AS (username text, password text, stream_id uuid);

CREATE OR REPLACE FUNCTION init_user(inputs init_user_input[]) RETURNS SETOF user_events LANGUAGE sql AS $$
  WITH nest AS (
    SELECT *, gen_salt('bf') AS salt FROM unnest(inputs)
  ), streams AS (
    INSERT INTO "user_streams" SELECT stream_id FROM nest RETURNING *
  )
  INSERT INTO "user_events" (stream_id, name, data)
  SELECT
    stream_id,
    'initialized',
    jsonb_build_object('username', username, 'salt', salt, 'passhash', crypt(password, salt))
  FROM nest RETURNING *;
$$;

DROP TYPE IF EXISTS set_username_input CASCADE;
CREATE TYPE set_username_input AS (stream_id uuid, username text);

CREATE OR REPLACE FUNCTION set_username(inputs set_username_input[]) RETURNS SETOF user_events LANGUAGE sql AS $$
  WITH nest AS (
    SELECT * FROM unnest(inputs)
  )
  INSERT INTO "user_events" (stream_id, name, data)
  SELECT stream_id, 'username_set', jsonb_build_object('username', username) FROM nest RETURNING *;
$$;

DROP TYPE IF EXISTS set_password_input CASCADE;
CREATE TYPE set_password_input AS (stream_id uuid, password text);

CREATE OR REPLACE FUNCTION set_password(inputs set_password_input[]) RETURNS SETOF user_events LANGUAGE sql AS $$
  WITH nest AS (
    SELECT *, gen_salt('bf') AS salt FROM unnest(inputs)
  )
  INSERT INTO "user_events" (stream_id, name, data)
  SELECT stream_id, 'password_set', jsonb_build_object('salt', salt, 'passhash', crypt(password, salt))
  FROM nest RETURNING *;
$$;
