create or replace function html(text) returns text language sql as $$
  select format($html$
    <!DOCTYPE html>
    <html>
      <head>
        <meta charset="utf-8" />
        <meta name="viewport" content="width=device-width,initial-scale=1" />
        <title>Tankard</title>
        <link
          rel="stylesheet"
          href="https://unpkg.com/@picocss/pico@2.0.6/css/pico.min.css"
          integrity="sha384-7P0NVe9LPDbUCAF+fH2R8Egwz1uqNH83Ns/bfJY0fN2XCDBMUI2S9gGzIOIRBKsA"
          crossorigin="anonymous"
        />
        <script
          src="https://unpkg.com/htmx.org@2.0.0"
          integrity="sha384-wS5l5IKJBvK6sPTKa2WZ1js3d947pvWXbPJ1OmWfEuxLgeHcEbjUUA5i9V5ZkpCw"
          crossorigin="anonymous"
        >
        </script>
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
  select html('<div hx-get="/users" hx-trigger="revealed"></div>');
$$;

create or replace function array_to_html(head text[], body anyarray) returns text language plpgsql as $$
declare
  html text := '<table><thead><tr><th scope=col>';
  x text[];
begin
  html := html || array_to_string(head, '</th><th scope=col>') || '</th></tr></thead><tbody>';
  foreach x slice 1 in array body loop
    html := html || '<tr><td>' || array_to_string(x, '</td><td>') || '</td></tr>';
  end loop;
  html := html || '</tbody></table>';
  return html;
end;
$$;
