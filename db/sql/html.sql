create or replace function html(text) returns text language sql as $$
  select format($html$
    <!DOCTYPE html>
    <html>
      <head>
        <meta charset="utf-8" />
        <meta name="viewport" content="width=device-width,initial-scale=1" />
        <title>Tankard</title>
        <link rel="stylesheet" href="pico.css" />
        <script src="htmx.js"></script>
        <script src="sse.js"></script>
      </head>
      <body>
        <header></header>
        <main class="container">%s</main>
        <footer></footer>
      </body>
    </html>
  $html$, $1);
$$;

create or replace function html_index() returns text language sql as $$
  select html($html$
    <div hx-ext="sse" sse-connect="/users_listen">
      <div hx-trigger="sse:users_event, revealed" hx-get="/users?select=id,username"></div>
    </div>
  $html$);
$$;

create or replace function trg_users_event () returns trigger language plpgsql as $$
begin
  perform pg_notify('users_event', '');
  return null;
end;
$$;

create or replace trigger trg_users_event after insert or update or delete on users execute function trg_users_event();
