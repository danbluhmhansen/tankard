use std::{
    error::Error,
    net::Ipv4Addr,
    time::{Duration, SystemTime},
};

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
use josekit::{
    jwe::{alg::rsaes::RsaesJweAlgorithm, JweContext, JweHeader},
    jws::{alg::rsassa::RsassaJwsAlgorithm, JwsHeader},
    jwt::JwtPayload,
};
use maud::{html, Markup, DOCTYPE};
use serde::Deserialize;
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
        form method="post" {
            input type="text" name="username" placeholder="Username" required class="bg-transparent";
            input type="password" name="password" placeholder="Password" required class="bg-transparent";
            button type="submit" name="submit" value="signin" { "Sign in" }
        }
        form method="post" {
            input type="text" name="username" placeholder="Username" required class="bg-transparent";
            input type="password" name="password" placeholder="Password" required class="bg-transparent";
            button type="submit" name="submit" value="signup" { "Sign up" }
        }
    })
}

async fn root_get(_: RootPath) -> Markup {
    root_page()
}

#[derive(Deserialize)]
struct RootPayload {
    username: String,
    password: String,
    submit: String,
}

async fn root_post(
    State(state): State<Pool<Postgres>>,
    jar: CookieJar,
    Form(form): Form<RootPayload>,
) -> Response {
    match form.submit.as_str() {
        "signin" => {
            let user = sqlx::query!("SELECT id FROM users WHERE username = $1;", form.username)
                .fetch_one(&state)
                .await
                .unwrap();

            let success =
                sqlx::query_scalar!("SELECT check_password($1, $2);", user.id, form.password)
                    .fetch_one(&state)
                    .await
                    .unwrap();

            if success.is_some_and(|s| s) {
                let mut header = JwsHeader::new();
                header.set_token_type("JWT");

                let now = SystemTime::now();
                let mut payload = JwtPayload::new();
                payload.set_issued_at(&now);
                payload.set_expires_at(&now.checked_add(Duration::from_secs(120)).unwrap());
                payload.set_subject(user.id.unwrap());

                let signer = RsassaJwsAlgorithm::Rs256
                    .signer_from_pem(std::env::var("ACCESS_TOKEN_PRIVATE_KEY").unwrap())
                    .unwrap();

                let token = josekit::jwt::encode_with_signer(&payload, &header, &signer).unwrap();

                let mut header = JweHeader::new();
                header.set_token_type("JWT");
                header.set_content_encryption("A128CBC-HS256");

                let encryptor = RsaesJweAlgorithm::RsaOaep
                    .encrypter_from_pem(std::env::var("ACCESS_TOKEN_PUBLIC_KEY").unwrap())
                    .unwrap();

                let token = JweContext::new()
                    .serialize_compact(token.as_bytes(), &header, &encryptor)
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
        "signup" => {
            let _ = sqlx::query!("SELECT init_user($1, $2);", form.username, form.password)
                .fetch_all(&state)
                .await;
            let _ = sqlx::query!("REFRESH MATERIALIZED VIEW users;")
                .fetch_all(&state)
                .await;

            root_page().into_response()
        }
        _ => unreachable!(),
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
