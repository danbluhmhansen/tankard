CREATE
OR REPLACE FUNCTION jsonb_deep_merge(jsonb, jsonb) RETURNS jsonb language SQL immutable AS $$
SELECT
  CASE
    jsonb_typeof($1)
    WHEN 'object' THEN CASE
      jsonb_typeof($2)
      WHEN 'object' THEN (
        SELECT
          jsonb_object_agg(
            k,
            CASE
              WHEN e2.v IS NULL THEN e1.v
              WHEN e1.v IS NULL THEN e2.v
              ELSE jsonb_deep_merge(e1.v, e2.v)
            END
          )
        FROM
          jsonb_each($1) e1(k, v) FULL
          JOIN jsonb_each($2) e2(k, v) USING (k)
      )
      ELSE $2
    END
    WHEN 'array' THEN (
      SELECT
        jsonb_agg(items.val)
      FROM
        (
          SELECT
            jsonb_array_elements($1) AS val
          UNION
          SELECT
            jsonb_array_elements($2) AS val
        ) AS items
    )
    ELSE $2
  END $$;

CREATE
OR REPLACE AGGREGATE jsonb_merge_agg(jsonb) (
  sfunc = jsonb_deep_merge,
  stype = jsonb,
  initcond = '{}'
);

DROP MATERIALIZED VIEW "users";

CREATE MATERIALIZED VIEW "users" AS
SELECT
  s.id,
  e.timestamp,
  e.data ->> 'username' AS username,
  e.data ->> 'salt' AS salt,
  e.data ->> 'passhash' AS passhash,
  e.data ->> 'email' AS email
FROM
  "event_streams" s
  JOIN (
    SELECT
      stream_id,
      max(timestamp) AS timestamp,
      jsonb_merge_agg(data) AS data
    FROM
      "events"
    GROUP BY
      stream_id
  ) e ON e.stream_id = s.id
WHERE
  s.type = 1;

CREATE
OR REPLACE FUNCTION init_user(
  username text,
  password text,
  stream_id uuid DEFAULT gen_random_uuid()
) RETURNS record language plpgsql AS $$ DECLARE salt text := gen_salt('bf');

user_event record;

BEGIN
INSERT INTO
  "event_streams" (id, TYPE)
VALUES
  (stream_id, 1);

INSERT INTO
  "events" (stream_id, name, data)
VALUES
  (
    stream_id,
    'user_initialized',
    jsonb_build_object(
      'username',
      username,
      'salt',
      salt,
      'passhash',
      crypt(password, salt)
    )
  ) RETURNING * INTO user_event;

RETURN user_event;

END $$;

CREATE
OR REPLACE FUNCTION set_username(stream_id uuid, username text) RETURNS record language plpgsql AS $$ BEGIN
INSERT INTO
  "events" (stream_id, name, data)
VALUES
  (
    stream_id,
    'username_set',
    jsonb_build_object('username', username)
  ) RETURNING *;

END $$;

CREATE
OR REPLACE FUNCTION set_password(stream_id uuid, password text) RETURNS record language plpgsql AS $$ DECLARE salt text := gen_salt('bf');

set_password_event record;

BEGIN
INSERT INTO
  "events" (stream_id, name, data)
VALUES
  (
    stream_id,
    'password_set',
    jsonb_build_object(
      'salt',
      salt,
      'passhash',
      crypt(password, salt)
    )
  ) RETURNING * INTO set_password_event;

RETURN set_password_event;

END $$;

CREATE
OR REPLACE FUNCTION check_password(id uuid, password text) RETURNS boolean language plpgsql AS $$ DECLARE salt text;

passhash text;

BEGIN
SELECT
  u.salt,
  u.passhash INTO salt,
  passhash
FROM
  users u
WHERE
  u.id = check_password.id
LIMIT
  1;

RETURN crypt(password, salt) = passhash;

END $$;
