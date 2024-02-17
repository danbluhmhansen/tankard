use std::{error::Error, net::Ipv4Addr, ops::Deref, time::Duration};

use apalis::{
    prelude::{Data, Job, Monitor, WorkerBuilder, WorkerFactoryFn},
    redis::RedisStorage,
    utils::TokioExecutor,
};
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
use tokio::{net::TcpListener, try_join};
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
#[cfg(debug_assertions)]
use tower_livereload::LiveReloadLayer;

mod components;
mod routes;

type AppState = Pool<Postgres>;

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

impl Job for DbJob {
    const NAME: &'static str = "db-job";
}

async fn db_worker(job: DbJob, pool: Data<AppState>) {
    match job {
        DbJob::InitGames {
            id,
            name,
            description,
        } => todo!(),
        DbJob::RefreshGameView => {
            let _ = sqlx::query!("REFRESH MATERIALIZED VIEW games;")
                .fetch_all(pool.deref())
                .await;
        }
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

    let store = RedisStorage::new(apalis::redis::connect(std::env::var("REDIS_URL")?).await?);

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
                .layer(Extension(store.clone())),
        )
        .with_state(pool.clone());

    #[cfg(debug_assertions)]
    let app = app.layer(LiveReloadLayer::new());

    let listener = TcpListener::bind((Ipv4Addr::new(127, 0, 0, 1), 1111)).await?;
    let http = async { axum::serve(listener, app).await };

    let monitor = async {
        Monitor::<TokioExecutor>::new()
            .register(
                WorkerBuilder::new("tankard")
                    .with_storage(store)
                    .data(pool)
                    .build_fn(db_worker),
            )
            .run()
            .await
    };

    try_join!(http, monitor)?;

    Ok(())
}
