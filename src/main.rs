use std::{error::Error, net::Ipv4Addr, time::Duration};

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    routing::post,
    Form, Router,
};
use axum_extra::{
    extract::{
        cookie::{Cookie, SameSite},
        CookieJar,
    },
    routing::{RouterExt, TypedPath},
};
use maud::{html, Markup, DOCTYPE};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;
#[cfg(debug_assertions)]
use tower_livereload::LiveReloadLayer;

fn layout(main: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html class="dark:text-white dark:bg-slate-900" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width,initial-scale=1";
                link rel="stylesheet" type="text/css" href="site.css";
                script
                    src="https://unpkg.com/htmx.org@1.9.10"
                    integrity="sha384-D1Kt99CQMDuVetoL1lrYwg5t+9QdHe7NLX/SoJYkXDFfX37iInKRy5xLSi8nO7UC"
                    crossorigin="anonymous" {}
            }
            body {
                header {
                    nav class="flex justify-center p-4" {
                        h1 { "Tankard" }
                    }
                }
                main class="container mx-auto" {
                    (main)
                }
            }
        }
    }
}

#[derive(TypedPath)]
#[typed_path("/")]
struct RootPath;

fn root_page() -> Markup {
    layout(html! {
        h1 { "Hello, World!" }
        form method="post" {
            input type="text" name="username" placeholder="Username" required class="bg-transparent";
            input type="password" name="password" placeholder="Password" required class="bg-transparent";
            input type="submit" value="Login";
        }
    })
}

async fn root_get(_: RootPath, jar: CookieJar) -> Markup {
    let token = jar.get("session_id").map(|c| c.value());
    println!("{token:?}");
    root_page()
}

#[derive(Deserialize)]
struct RootPayload {
    username: String,
    password: String,
}

#[derive(Deserialize, Serialize)]
struct Claims {
    exp: usize,
    aud: Option<String>,
    iat: Option<usize>,
    iss: Option<String>,
    nbf: Option<usize>,
    sub: Option<String>,
}

impl Claims {
    fn new(exp: usize) -> Self {
        Self {
            exp,
            aud: None,
            iat: None,
            iss: None,
            nbf: None,
            sub: None,
        }
    }

    fn sub(mut self, sub: String) -> Self {
        self.sub = Some(sub);
        self
    }
}

async fn root_post(
    State(state): State<Pool<Postgres>>,
    jar: CookieJar,
    Form(form): Form<RootPayload>,
) -> Response {
    let user = sqlx::query!("SELECT id FROM users WHERE username = $1;", form.username)
        .fetch_one(&state)
        .await
        .unwrap();

    let success = sqlx::query_scalar!("SELECT check_password($1, $2);", user.id, form.password)
        .fetch_one(&state)
        .await
        .unwrap();

    if success.is_some_and(|s| s) {
        let claims = Claims::new(300).sub(user.id.unwrap().into());
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret("secret".as_ref()),
        )
        .unwrap();
        (
            jar.add(
                Cookie::build(("session_id", token))
                    .http_only(true)
                    .same_site(SameSite::Strict)
                    .build(),
            ),
            root_page(),
        )
            .into_response()
    } else {
        root_page().into_response()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let conn_str = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&conn_str)
        .await?;

    let app = Router::new()
        .route("/", post(root_post))
        .typed_get(root_get)
        .fallback_service(ServeDir::new("static"))
        .with_state(pool);

    #[cfg(debug_assertions)]
    let app = app.layer(LiveReloadLayer::new());

    let listener = TcpListener::bind((Ipv4Addr::new(127, 0, 0, 1), 1111)).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
