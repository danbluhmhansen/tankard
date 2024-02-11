create extension if not exists pgcrypto;

create
or replace function jsonb_deep_merge (jsonb, jsonb) returns jsonb language sql immutable as $$
  select
    case
      jsonb_typeof($1)
      when 'object' then case
        jsonb_typeof($2)
        when 'object' then (
          select
            jsonb_object_agg(
              k,
              case
                when e2.v is null then e1.v
                when e1.v is null then e2.v
                else jsonb_deep_merge(e1.v, e2.v)
              end
            )
          from
            jsonb_each($1) e1(k, v) full
            join jsonb_each($2) e2(k, v) using (k)
        )
        else coalesce($2, $1)
      end
      when 'array' then (
        select
          jsonb_agg(items.val)
        from
          (
            select
              jsonb_array_elements($1) as val
            union
            select
              jsonb_array_elements($2) as val
          ) as items
      )
      else $2
  end
$$;

create
or replace aggregate jsonb_merge_agg (jsonb) (
  sfunc = jsonb_deep_merge,
  stype = jsonb,
  initcond = '{}'
);
