use std::{error::Error, net::Ipv4Addr, time::Duration};

use axum::{
    extract::{Request, State},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Extension, Form, Router,
};
use axum_extra::{
    extract::{
        cookie::{Cookie, SameSite},
        CookieJar,
    },
    routing::{RouterExt, TypedPath},
};
use maud::{html, Markup, DOCTYPE};
use pasetors::{
    claims::{Claims, ClaimsValidationRules},
    keys::SymmetricKey,
    local,
    token::UntrustedToken,
    version4::V4,
    Local,
};
use serde::Deserialize;
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
#[cfg(debug_assertions)]
use tower_livereload::LiveReloadLayer;

#[derive(Clone, Debug)]
struct CurrentUser {
    id: String,
}

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

async fn root_get(_: RootPath, Extension(current_user): Extension<Option<CurrentUser>>) -> Markup {
    println!("{current_user:?}");
    root_page()
}

#[derive(Deserialize)]
struct RootPayload {
    username: String,
    password: String,
    submit: String,
}

async fn root_post(
    _: RootPath,
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
                let mut claims = Claims::new_expires_in(&Duration::from_secs(120)).unwrap();
                claims
                    .subject(user.id.unwrap().to_string().as_str())
                    .unwrap();

                let token = local::encrypt(
                    &SymmetricKey::<V4>::try_from(std::env::var("PASERK").unwrap().as_str())
                        .unwrap(),
                    &claims,
                    None,
                    None,
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

async fn auth(jar: CookieJar, mut req: Request, next: Next) -> Response {
    if let Some(id) = jar
        .get("session_id")
        .map(|c| c.value())
        .and_then(|token| {
            local::decrypt(
                &SymmetricKey::<V4>::try_from(std::env::var("PASERK").unwrap().as_str()).unwrap(),
                &UntrustedToken::<Local, V4>::try_from(token).unwrap(),
                &ClaimsValidationRules::new(),
                None,
                None,
            )
            .ok()
        })
        .and_then(|token| {
            token
                .payload_claims()
                .and_then(|c| c.get_claim("sub"))
                .map(|v| v.to_string())
        })
    {
        req.extensions_mut().insert(Some(CurrentUser { id }));
    }

    next.run(req).await
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
        .typed_get(root_get)
        .typed_post(root_post)
        .fallback_service(ServeDir::new("static"))
        .layer(
            ServiceBuilder::new()
                .layer(Extension(None::<CurrentUser>))
                .layer(middleware::from_fn(auth)),
        )
        .with_state(pool);

    #[cfg(debug_assertions)]
    let app = app.layer(LiveReloadLayer::new());

    let listener = TcpListener::bind((Ipv4Addr::new(127, 0, 0, 1), 1111)).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
