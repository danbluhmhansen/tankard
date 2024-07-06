select
    table_name::text,
    jsonb_agg(jsonb_build_object('column_name', column_name, 'data_type', data_type)) as columns
from information_schema.columns
where table_schema = 'public'
group by table_name;
