use pgrx::prelude::*;

#[pg_extern]
fn html_doc(children: &str) -> String {
    maud::html! {
        (maud::DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width,initial-scale=1";
                title { "Tankard" }
                link
                  rel="stylesheet"
                  href="https://unpkg.com/@picocss/pico@2.0.6/css/pico.min.css"
                  integrity="sha384-7P0NVe9LPDbUCAF+fH2R8Egwz1uqNH83Ns/bfJY0fN2XCDBMUI2S9gGzIOIRBKsA"
                  crossorigin="anonymous";
                script
                    src="https://unpkg.com/htmx.org@2.0.0"
                    integrity="sha384-wS5l5IKJBvK6sPTKa2WZ1js3d947pvWXbPJ1OmWfEuxLgeHcEbjUUA5i9V5ZkpCw"
                    crossorigin="anonymous" {}
            }
            body { main { (maud::PreEscaped(children)) } }
        }
    }
    .0
}

#[pg_extern]
fn html_users() -> Result<String, spi::Error> {
    let usernames = Spi::connect(|client| -> Result<Vec<String>, spi::Error> {
        Ok(client
            .select("select username from users;", None, None)?
            .filter_map(|row| row["username"].value().ok().flatten())
            .collect())
    })?;
    Ok(maud::html! {
        table {
            thead { tr { th { "Username" } } }
            tbody { @for username in usernames { tr { td { (username) } } } }
        }
    }
    .0)
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
pub(crate) mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn test_html_doc() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;
        Spi::run("insert into users (username, salt, passhash) values ('one', '', ''), ('two', '', ''), ('three', '', '');")?;

        // assert_eq!(Some(""), Spi::get_one("select html_doc(html_users());")?);

        Ok(())
    }
}
