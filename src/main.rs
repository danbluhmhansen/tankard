use std::{error::Error, net::Ipv4Addr, time::Duration};

use axum::{
    extract::Request,
    middleware::{self, Next},
    response::Response,
    Extension, Router,
};
use axum_extra::{extract::CookieJar, routing::RouterExt};
use maud::{html, Markup, DOCTYPE};
use pasetors::{
    claims::ClaimsValidationRules, keys::SymmetricKey, local, token::UntrustedToken, version4::V4,
    Local,
};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
#[cfg(debug_assertions)]
use tower_livereload::LiveReloadLayer;

mod routes;

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
        .typed_get(routes::index::get)
        .typed_post(routes::index::post)
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
