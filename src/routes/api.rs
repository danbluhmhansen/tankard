use axum::{response::IntoResponse, routing, Extension, Router};
use axum_streams::StreamBodyAs;
use serde::Serialize;
use sqlx::PgPool;
use tokio_stream::StreamExt;
use uuid::Uuid;

pub(crate) fn route() -> Router {
    Router::new().route("/api/games", routing::get(games))
}

#[derive(Serialize)]
struct Game {
    id: Uuid,
    name: String,
    description: Option<String>,
}

async fn games(Extension(pool): Extension<&'static PgPool>) -> impl IntoResponse {
    let games = sqlx::query_as!(Game, "select id, name, description from games;").fetch(pool);
    StreamBodyAs::json_array(games.filter_map(|g| g.ok()))
}
