use std::{error::Error, net::Ipv4Addr, sync::Arc, time::Duration};

use axum::{
    extract::Request,
    middleware::{self, Next},
    response::Response,
    Extension, Router,
};
use axum_extra::extract::CookieJar;
use pasetors::{
    claims::ClaimsValidationRules, keys::SymmetricKey, local, token::UntrustedToken, version4::V4,
    Local,
};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, types::Uuid, Pool, Postgres};
use tokio::{
    join,
    net::TcpListener,
    sync::{broadcast, mpsc},
};
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
#[cfg(debug_assertions)]
use tower_livereload::LiveReloadLayer;

mod components;
mod routes;

#[derive(Clone, Debug)]
pub struct AppState {
    pool: Pool<Postgres>,
}

impl AppState {
    fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }
}

#[derive(Clone, Debug)]
struct CurrentUser {
    id: Uuid,
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
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
        })
    {
        req.extensions_mut().insert(Some(CurrentUser { id }));
    } else {
        req.extensions_mut().insert(None::<CurrentUser>);
    }
    next.run(req).await
}

#[derive(Debug, Deserialize, Serialize)]
enum DbJob {
    InitGames {
        id: String,
        name: String,
        description: Option<String>,
    },
    RefreshGameView,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
enum SseJob {
    RefreshGameView,
}

#[cfg(debug_assertions)]
fn not_htmx_predicate<T>(req: &Request<T>) -> bool {
    !req.headers().contains_key("hx-request")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let conn_str = std::env::var("DATABASE_URL")?;
    let state = Arc::new(AppState::new(
        PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(3))
            .connect(&conn_str)
            .await?,
    ));

    let (tx, mut rx) = mpsc::channel::<DbJob>(32);
    let (sse_tx, _) = broadcast::channel::<SseJob>(32);

    let app = Router::new()
        .merge(routes::games::route())
        .merge(routes::index::route())
        .merge(routes::profile::route())
        .merge(routes::signin::route())
        .merge(routes::signout::route())
        .merge(routes::signup::route())
        .fallback_service(ServeDir::new("dist"))
        .layer(
            ServiceBuilder::new()
                .layer(middleware::from_fn(auth))
                .layer(Extension(tx.clone()))
                .layer(Extension(sse_tx.clone())),
        )
        .with_state(state.clone());

    #[cfg(debug_assertions)]
    let app = app.layer(LiveReloadLayer::new().request_predicate(not_htmx_predicate));

    let listener = TcpListener::bind((Ipv4Addr::new(127, 0, 0, 1), 1111)).await?;
    let http = async { axum::serve(listener, app).await };

    let monitor = async move {
        while let Some(job) = rx.recv().await {
            match job {
                DbJob::InitGames {
                    id: _,
                    name: _,
                    description: _,
                } => todo!(),
                DbJob::RefreshGameView => {
                    let _ = sqlx::query!("REFRESH MATERIALIZED VIEW games;")
                        .fetch_all(&state.pool)
                        .await;
                    let _ = sse_tx.send(SseJob::RefreshGameView);
                }
            }
        }
    };

    let _ = join!(http, monitor);

    Ok(())
}
