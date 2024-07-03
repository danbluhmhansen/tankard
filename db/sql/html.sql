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

create or replace function array_to_html(head text[], body anyarray) returns text language plpgsql as $$
declare
  html text := '<table><thead><tr><th scope=col>';
  x text[];
begin
  if head is null or body is null then
    return '';
  end if;

  html := html || array_to_string(head, '</th><th scope=col>') || '</th></tr></thead><tbody>';
  foreach x slice 1 in array body loop
    html := html || '<tr><td>' || array_to_string(x, '</td><td>') || '</td></tr>';
  end loop;
  html := html || '</tbody></table>';
  return html;
end;
$$;

create or replace function trg_users_event () returns trigger language plpgsql as $$
begin
  perform pg_notify('users_event', '');
  return null;
end;
$$;

create or replace trigger trg_users_event after insert or update or delete on users execute function trg_users_event();
