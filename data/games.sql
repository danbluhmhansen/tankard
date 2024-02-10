DROP MATERIALIZED VIEW IF EXISTS "games";
CREATE MATERIALIZED VIEW "games" AS
  SELECT
    s.id,
    e.added,
	e.updated,
    e.data ->> 'name' AS name,
    e.data ->> 'description' AS description
  FROM
    "game_streams" s
    JOIN (
      SELECT
        stream_id,
		min(timestamp) AS added,
        max(timestamp) AS updated,
        jsonb_merge_agg(data) AS data
      FROM
        "game_events"
      GROUP BY
        stream_id
    ) e ON e.stream_id = s.id;

DROP TYPE IF EXISTS init_game_input CASCADE;
CREATE TYPE init_game_input AS (name text, description text, stream_id uuid);

CREATE OR REPLACE FUNCTION init_game(inputs init_game_input[]) RETURNS SETOF game_events LANGUAGE sql AS $$
  WITH nest AS (
    SELECT * FROM unnest(inputs)
  ), streams AS (
    INSERT INTO "game_streams" SELECT stream_id FROM nest RETURNING *
  )
  INSERT INTO "game_events" (stream_id, name, data)
  SELECT stream_id, 'initialized', jsonb_build_object('name', name, 'description', description)
  FROM nest RETURNING *;
$$;

