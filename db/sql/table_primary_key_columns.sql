select kcu.column_name::text, c.data_type::text
from information_schema.table_constraints tc
join information_schema.key_column_usage kcu using
  (constraint_catalog, constraint_schema, constraint_name, table_catalog, table_schema, table_name)
join information_schema.columns c using (table_catalog, table_schema, table_name, column_name)
where constraint_type = 'PRIMARY KEY' and table_name = $1;
