use std::{error::Error, net::Ipv4Addr, time::Duration};

use axum::{
    extract::Request,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    Router,
};
use axum_extra::extract::{
    cookie::{Cookie, SameSite},
    CookieJar,
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
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;
#[cfg(debug_assertions)]
use tower_livereload::LiveReloadLayer;

mod routes;

type AppState = Pool<Postgres>;

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

async fn sign_in<'a>(
    pool: &Pool<Postgres>,
    username: String,
    password: String,
) -> Result<Option<Cookie<'a>>, Box<dyn Error>> {
    let user = sqlx::query!(
        "SELECT id, check_password(id, $2) FROM users WHERE username = $1 LIMIT 1;",
        username,
        password
    )
    .fetch_one(pool)
    .await?;

    if let Some(id) = user
        .id
        .zip(user.check_password)
        .filter(|(_, c)| *c)
        .map(|(id, _)| id.to_string())
    {
        let exp = Duration::from_secs(120);
        let mut claims = Claims::new_expires_in(&exp)?;
        claims.subject(id.as_str())?;

        let token = local::encrypt(
            &SymmetricKey::<V4>::try_from(std::env::var("PASERK")?.as_str())?,
            &claims,
            None,
            None,
        )?;

        Ok(Some(
            Cookie::build(("session_id", token))
                .max_age(exp.try_into()?)
                .http_only(true)
                .same_site(SameSite::Strict)
                .build(),
        ))
    } else {
        Ok(None)
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
        req.extensions_mut().insert(CurrentUser { id });
        next.run(req).await
    } else {
        Redirect::to("/").into_response()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let conn_str = std::env::var("DATABASE_URL")?;
    let pool: AppState = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&conn_str)
        .await?;

    let app = Router::new()
        .merge(routes::index::route())
        .merge(routes::profile::route())
        .fallback_service(ServeDir::new("static"))
        .with_state(pool);

    #[cfg(debug_assertions)]
    let app = app.layer(LiveReloadLayer::new());

    let listener = TcpListener::bind((Ipv4Addr::new(127, 0, 0, 1), 1111)).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
