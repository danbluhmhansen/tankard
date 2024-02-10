CREATE OR REPLACE FUNCTION check_password(id uuid, password text) RETURNS boolean LANGUAGE sql AS $$
  WITH hash AS (
    SELECT salt, passhash FROM users WHERE id = id LIMIT 1
  )
  SELECT crypt(password, salt) = passhash FROM hash LIMIT 1;
$$;
