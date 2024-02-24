use amqprs::{
    channel::{BasicConsumeArguments, BasicPublishArguments, Channel},
    BasicProperties,
};
use axum::{
    http::StatusCode,
    middleware,
    response::{
        sse::{Event, KeepAlive},
        IntoResponse, Redirect, Response, Sse,
    },
    Extension, Form, Router,
};
use axum_extra::routing::{RouterExt, TypedPath};
use axum_htmx::{HxBoosted, HxRequest};
use maud::{html, Markup};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use tokio_stream::{wrappers::UnboundedReceiverStream, StreamExt};
use uuid::Uuid;

use crate::{
    auth::{self, CurrentUser},
    components::boost,
};

pub(crate) fn route() -> Router {
    Router::new()
        .typed_get(partial)
        .typed_get(get)
        .typed_post(post)
        .typed_get(sse)
        .layer(middleware::from_fn(auth::req_auth))
}

pub(crate) async fn table(user_id: Uuid, pool: &Pool<Postgres>) -> Markup {
    let games = sqlx::query!("SELECT name FROM games WHERE user_id = $1;", user_id)
        .fetch_all(pool)
        .await;
    html! {
        @if let Ok(games) = games {
            table hx-ext="sse" sse-connect=(SsePath) sse-swap="rfsh-games" {
                thead { tr { th { "Name" } } }
                tbody class="text-center" {
                    @for game in games { tr { td { @if let Some(name) = game.name { (name) } } } }
                }
            }
        } @else {
            p hx-ext="sse" sse-connect=(SsePath) sse-swap="rfsh-games" { "No games..." }
        }
    }
}

pub(crate) async fn page(is_hx: bool, user_id: Uuid, pool: &Pool<Postgres>) -> Markup {
    html! {
        dialog #add class="inset-0 justify-center items-center w-full h-full target:flex bg-black/50 backdrop-blur-sm" {
            #dialog class="flex z-10 flex-col gap-4 p-4 max-w-sm bg-white rounded border dark:text-white dark:bg-slate-900" {
                form method="post" class="flex flex-col gap-2" {
                    input
                        type="text"
                        name="name"
                        placeholder="Name"
                        required
                        class="p-1 bg-transparent border border-black dark:border-white";
                    input
                        type="textarea"
                        name="description"
                        placeholder="Description"
                        class="p-1 bg-transparent border border-black dark:border-white";
                    button type="submit" { "Add" }
                }
            }
            a href="#!" hx-boost="false" class="fixed inset-0" {}
        }
        #games hx-get=(PartialPath) hx-trigger="revealed" class="flex justify-center" {
            @if is_hx { "..." } @else { (table(user_id, pool).await) }
        }
        a href="#add" hx-boost="false" class="text-center" { "Add" }
    }
}

#[derive(TypedPath)]
#[typed_path("/games-prtl")]
pub(crate) struct PartialPath;

pub(crate) async fn partial(
    _: PartialPath,
    HxRequest(is_hx): HxRequest,
    Extension(CurrentUser { id }): Extension<CurrentUser>,
    Extension(pool): Extension<Pool<Postgres>>,
) -> Response {
    if is_hx {
        table(id, &pool).await.into_response()
    } else {
        Redirect::to(Path.to_uri().path()).into_response()
    }
}

#[derive(TypedPath)]
#[typed_path("/games")]
pub(crate) struct Path;

pub(crate) async fn get(
    _: Path,
    HxRequest(is_hx): HxRequest,
    HxBoosted(boosted): HxBoosted,
    Extension(CurrentUser { id }): Extension<CurrentUser>,
    Extension(pool): Extension<Pool<Postgres>>,
) -> Markup {
    boost(page(is_hx, id, &pool).await, true, boosted)
}

#[derive(Deserialize)]
pub(crate) struct Payload {
    name: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct InitGame {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
}

impl InitGame {
    pub(crate) fn new(id: Uuid, name: String, description: Option<String>) -> Self {
        Self {
            id,
            name,
            description,
        }
    }
}

pub(crate) async fn post(
    _: Path,
    HxRequest(is_hx): HxRequest,
    HxBoosted(boosted): HxBoosted,
    Extension(CurrentUser { id }): Extension<CurrentUser>,
    Extension(pool): Extension<Pool<Postgres>>,
    Extension(channel): Extension<Channel>,
    Form(Payload { name, description }): Form<Payload>,
) -> Markup {
    if let Ok(init_game) = serde_json::to_vec(&InitGame::new(id, name, description)) {
        let _ = channel
            .basic_publish(
                BasicProperties::default(),
                init_game,
                BasicPublishArguments::new("", "db"),
            )
            .await;
    }
    boost(page(is_hx, id, &pool).await, true, boosted)
}

#[derive(TypedPath)]
#[typed_path("/games-sse")]
pub(crate) struct SsePath;

pub(crate) async fn sse(
    _: SsePath,
    Extension(CurrentUser { id }): Extension<CurrentUser>,
    Extension(pool): Extension<Pool<Postgres>>,
    Extension(channel): Extension<Channel>,
) -> Response {
    if let Ok((_, rx)) = channel
        .basic_consume_rx(
            BasicConsumeArguments::new("sse", "")
                .auto_ack(true)
                .finish(),
        )
        .await
    {
        let table = table(id, &pool).await;

        let stream = UnboundedReceiverStream::new(rx).map(move |msg| {
            if let Some("rfsh-games") = msg
                .content
                .as_ref()
                .and_then(|content| std::str::from_utf8(content).ok())
            {
                Ok(Event::default()
                    .event("rfsh-games")
                    .data(table.clone().into_string()))
            } else {
                Err("cannot parse content")
            }
        });

        Sse::new(stream)
            .keep_alive(KeepAlive::default())
            .into_response()
    } else {
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    }
}
