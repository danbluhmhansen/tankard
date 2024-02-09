use std::{error::Error, net::Ipv4Addr, time::Duration};

use axum::{
    extract::Request,
    middleware::{self, Next},
    response::Response,
    Router,
};
use axum_extra::extract::CookieJar;
use pasetors::{
    claims::ClaimsValidationRules, keys::SymmetricKey, local, token::UntrustedToken, version4::V4,
    Local,
};
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;
#[cfg(debug_assertions)]
use tower_livereload::LiveReloadLayer;

mod components;
mod routes;

type AppState = Pool<Postgres>;

#[derive(Clone, Debug)]
struct CurrentUser {
    id: String,
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
    } else {
        req.extensions_mut().insert(None::<CurrentUser>);
    }
    next.run(req).await
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
        .merge(routes::signin::route())
        .merge(routes::signout::route())
        .merge(routes::signup::route())
        .fallback_service(ServeDir::new("static"))
        .layer(middleware::from_fn(auth))
        .with_state(pool);

    #[cfg(debug_assertions)]
    let app = app.layer(LiveReloadLayer::new());

    let listener = TcpListener::bind((Ipv4Addr::new(127, 0, 0, 1), 1111)).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
