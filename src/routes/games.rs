use amqprs::channel::{BasicConsumeArguments, Channel};
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
use bincode::error::DecodeError;
use maud::{html, Markup};
use serde::Deserialize;
use sqlx::{Pool, Postgres};
use tokio_stream::{wrappers::UnboundedReceiverStream, StreamExt};
use uuid::Uuid;

use crate::{
    auth::{self, CurrentUser},
    commands::{Command, InitGame},
    components::boost,
    Exchange, Queue,
};

pub(crate) fn route() -> Router {
    Router::new()
        .typed_get(partial)
        .typed_get(get)
        .typed_post(post)
        .typed_get(sse)
        .layer(middleware::from_fn(auth::req_auth))
}

const SSE_EVENT: &str = "rfsh-games";

pub(crate) async fn table(user_id: Uuid, pool: &Pool<Postgres>) -> Markup {
    let games = sqlx::query!("SELECT name FROM games WHERE user_id = $1;", user_id)
        .fetch_all(pool)
        .await;
    html! {
        @if let Ok(games) = games {
            table hx-ext="sse" sse-connect=(SsePath) sse-swap=(SSE_EVENT) {
                thead { tr { th { "Name" } } }
                tbody {
                    @for game in games { tr { td { @if let Some(name) = game.name { (name) } } } }
                }
            }
        } @else {
            p hx-ext="sse" sse-connect=(SsePath) sse-swap=(SSE_EVENT) { "No games..." }
        }
    }
}

pub(crate) async fn page(is_hx: bool, user_id: Uuid, pool: &Pool<Postgres>) -> Markup {
    html! {
        dialog #add {
            #dialog {
                form method="post" {
                    input
                        type="text"
                        name="name"
                        placeholder="Name"
                        required;
                    input
                        type="textarea"
                        name="description"
                        placeholder="Description";
                    button type="submit" { "Add" }
                }
            }
            a href="#!" hx-boost="false" {}
        }
        #games hx-get=(PartialPath) hx-trigger="revealed" {
            @if is_hx { "..." } @else { (table(user_id, pool).await) }
        }
        a href="#add" hx-boost="false" { "Add" }
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

pub(crate) async fn post(
    _: Path,
    HxRequest(is_hx): HxRequest,
    HxBoosted(boosted): HxBoosted,
    Extension(CurrentUser { id }): Extension<CurrentUser>,
    Extension(pool): Extension<Pool<Postgres>>,
    Extension(channel): Extension<Channel>,
    Form(Payload { name, description }): Form<Payload>,
) -> Markup {
    let _ = Command::InitGame(InitGame {
        user_id: id,
        name,
        description,
    })
    .publish(&channel, Queue::Db, Exchange::Default)
    .await;
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
            BasicConsumeArguments::new(Queue::Sse.into(), "")
                .auto_ack(true)
                .finish(),
        )
        .await
    {
        let table = table(id, &pool).await.into_string();

        let stream = UnboundedReceiverStream::new(rx).map(move |msg| {
            match msg.content.as_ref().map(|content| {
                bincode::serde::decode_from_slice(content, bincode::config::standard())
            }) {
                Some(Ok((Command::RefreshGames, _))) => {
                    Ok(Event::default().event(SSE_EVENT).data(table.clone()))
                }
                Some(Err(err)) => Err(err),
                _ => Err(DecodeError::Other("empty message")),
            }
        });

        Sse::new(stream)
            .keep_alive(KeepAlive::default())
            .into_response()
    } else {
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    }
}
