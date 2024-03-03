use amqprs::channel::Channel;
use axum::{middleware, response::IntoResponse, Extension, Json, Router};
use axum_extra::{
    extract::Query,
    routing::{RouterExt, TypedPath},
};
use axum_streams::StreamBodyAs;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio_stream::StreamExt;
use uuid::Uuid;

use crate::{
    auth::{self, CurrentUser},
    commands::{Command, InitGame, SetGame},
    Exchange, Queue,
};

pub(crate) fn route() -> Router {
    Router::new()
        .typed_get(get)
        .typed_post(post)
        .typed_put(put)
        .typed_delete(delete)
        .layer(middleware::from_fn(auth::req_auth))
}

#[derive(TypedPath)]
#[typed_path("/api/games")]
pub(crate) struct Path;

#[derive(Serialize)]
struct Get {
    id: Uuid,
    name: String,
    description: Option<String>,
}

async fn get(
    _: Path,
    Extension(CurrentUser { id }): Extension<CurrentUser>,
    Extension(pool): Extension<&'static PgPool>,
) -> impl IntoResponse {
    let games = sqlx::query_as!(
        Get,
        "select id, name, description from games where user_id = $1;",
        id
    )
    .fetch(pool);
    StreamBodyAs::json_array(games.filter_map(|g| g.ok()))
}

#[derive(Debug, Deserialize)]
struct Post {
    id: Uuid,
    name: String,
    description: Option<String>,
}

async fn post(
    _: Path,
    Extension(CurrentUser { id: user_id }): Extension<CurrentUser>,
    Extension(channel): Extension<Channel>,
    Json(payload): Json<Vec<Post>>,
) {
    let _ = Command::InitGames(
        payload
            .into_iter()
            .map(
                |Post {
                     id,
                     name,
                     description,
                 }| InitGame {
                    id,
                    user_id,
                    name,
                    description,
                },
            )
            .collect(),
    )
    .publish(&channel, Queue::Db, Exchange::Default)
    .await;
}

#[derive(Debug, Deserialize)]
struct Put {
    id: Uuid,
    name: Option<String>,
    description: Option<Option<String>>,
}

async fn put(_: Path, Extension(channel): Extension<Channel>, Json(payload): Json<Vec<Put>>) {
    let _ = Command::SetGames(
        payload
            .into_iter()
            .map(
                |Put {
                     id,
                     name,
                     description,
                 }| SetGame {
                    id,
                    name,
                    description,
                },
            )
            .collect(),
    )
    .publish(&channel, Queue::Db, Exchange::Default)
    .await;
}

#[derive(Deserialize)]
struct Delete {
    #[serde(default)]
    ids: Vec<Uuid>,
}

async fn delete(
    _: Path,
    Extension(channel): Extension<Channel>,
    Query(Delete { ids }): Query<Delete>,
) {
    let _ = Command::DropGames(ids)
        .publish(&channel, Queue::Db, Exchange::Default)
        .await;
}
