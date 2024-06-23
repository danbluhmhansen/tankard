select column_name::text, data_type::text
from information_schema.columns
where table_name = $1
and column_name not in ('added', 'updated')
and column_name not in (
    select pg_attribute.attname
    from pg_index, pg_class, pg_attribute
    where pg_class.oid = $1::regclass
    and indrelid = pg_class.oid
    and pg_attribute.attrelid = pg_class.oid
    and pg_attribute.attnum = any(pg_index.indkey)
    and indisprimary
);
