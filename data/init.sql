CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE OR REPLACE FUNCTION jsonb_deep_merge(jsonb, jsonb) RETURNS jsonb language SQL immutable AS $$
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
  END
$$;

CREATE OR REPLACE AGGREGATE jsonb_merge_agg(jsonb) (
  sfunc = jsonb_deep_merge,
  stype = jsonb,
  initcond = '{}'
);
