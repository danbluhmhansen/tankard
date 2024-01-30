use std::{error::Error, net::Ipv4Addr, time::Duration};

use axum::{response::Html, Router};
use axum_extra::routing::{RouterExt, TypedPath};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;

#[derive(TypedPath)]
#[typed_path("/")]
struct RootPath;

async fn root_get(_: RootPath) -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let conn_str = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&conn_str)
        .await?;
    let app = Router::new().typed_get(root_get).with_state(pool);
    let listener = TcpListener::bind((Ipv4Addr::new(127, 0, 0, 1), 1111)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
