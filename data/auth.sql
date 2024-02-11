create
or replace function check_password (id uuid, password text) returns boolean language sql as $$
  with hash as (
    select salt, passhash from users where id = id limit 1
  )
  select crypt(password, salt) = passhash from hash limit 1;
$$;
