use std::sync::Arc;

use axum::{
    extract::State,
    response::{
        sse::{Event, KeepAlive},
        IntoResponse, Redirect, Response, Sse,
    },
    Extension, Form, Router,
};
use axum_extra::routing::{RouterExt, TypedPath};
use axum_htmx::{HxBoosted, HxRequest};
use futures_util::{FutureExt, Stream, StreamExt};
use lapin::{
    options::{BasicConsumeOptions, BasicPublishOptions},
    types::FieldTable,
    BasicProperties, Channel,
};
use maud::{html, Markup};
use serde::Deserialize;
use sqlx::{types::Uuid, Pool, Postgres};

use crate::{components::boost, AppState, CurrentUser};

use super::index;

pub(crate) fn route() -> Router<Arc<AppState>> {
    Router::new()
        .typed_get(partial)
        .typed_get(get)
        .typed_post(post)
        .typed_get(sse)
}

pub(crate) async fn table(user_id: Uuid, pool: Pool<Postgres>) -> Markup {
    let games = sqlx::query!("SELECT name FROM games WHERE user_id = $1;", user_id)
        .fetch_all(&pool)
        .await;
    html! {
        @if let Ok(games) = games {
            table #games hx-ext="sse" sse-connect=(SsePath.to_uri().path()) sse-swap="rfsh-games" {
                thead { tr { th { "Name" } } }
                tbody {
                    @for game in games { tr { td { @if let Some(name) = game.name { (name) } } } }
                }
            }
        } @else {
            p #games hx-ext="sse" sse-connect=(SsePath.to_uri().path()) sse-swap="rfsh-games" { "No games..." }
        }
    }
}

pub(crate) async fn page(is_hx: bool, user_id: Uuid, pool: Pool<Postgres>) -> Markup {
    html! {
        @if is_hx {
            #games
                hx-get=(PartialPath.to_uri().path())
                hx-trigger="revealed"
                hx-select="#games"
                hx-target="this"
                hx-swap="outerHTML" {
                "..."
            }
        } @else {
            (table(user_id, pool).await)
        }
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
}

#[derive(TypedPath)]
#[typed_path("/games-prtl")]
pub(crate) struct PartialPath;

pub(crate) async fn partial(
    _: PartialPath,
    HxRequest(is_hx): HxRequest,
    Extension(user): Extension<Option<CurrentUser>>,
    State(state): State<Arc<AppState>>,
) -> Response {
    if let Some(CurrentUser { id }) = user {
        if is_hx {
            table(id, state.pool.clone()).await.into_response()
        } else {
            Redirect::to(Path.to_uri().path()).into_response()
        }
    } else {
        Redirect::to(index::Path.to_uri().path()).into_response()
    }
}

#[derive(TypedPath)]
#[typed_path("/games")]
pub(crate) struct Path;

pub(crate) async fn get(
    _: Path,
    HxRequest(is_hx): HxRequest,
    HxBoosted(boosted): HxBoosted,
    Extension(user): Extension<Option<CurrentUser>>,
    State(state): State<Arc<AppState>>,
) -> Response {
    if let Some(CurrentUser { id }) = user {
        boost(page(is_hx, id, state.pool.clone()).await, true, boosted).into_response()
    } else {
        Redirect::to(index::Path.to_uri().path()).into_response()
    }
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
    Extension(user): Extension<Option<CurrentUser>>,
    Extension(channel): Extension<Channel>,
    State(state): State<Arc<AppState>>,
    Form(Payload { name, description }): Form<Payload>,
) -> Response {
    if let Some(CurrentUser { id }) = user {
        let _ = sqlx::query!(
            "SELECT id FROM init_games(ARRAY[ROW($1, $2, $3, gen_random_uuid())]::init_games_input[]);",
            id,
            name,
            description
        )
        .fetch_all(&state.pool)
        .await;
        let _ = channel
            .basic_publish(
                "",
                "db",
                BasicPublishOptions::default(),
                "rfsh-games".as_bytes(),
                BasicProperties::default(),
            )
            .await;
        boost(page(is_hx, id, state.pool.clone()).await, true, boosted).into_response()
    } else {
        Redirect::to(index::Path.to_uri().path()).into_response()
    }
}

#[derive(TypedPath)]
#[typed_path("/games-sse")]
pub(crate) struct SsePath;

pub(crate) async fn sse(
    _: SsePath,
    Extension(user): Extension<Option<CurrentUser>>,
    Extension(channel): Extension<Channel>,
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, lapin::Error>>> {
    let stream = channel
        .basic_consume(
            "sse",
            "sse-consumer",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .unwrap()
        .then(move |j| {
            table(user.as_ref().unwrap().id, state.pool.clone()).map(move |t| match j {
                Ok(_) => Ok(Event::default().event("rfsh-games").data(t.into_string())),
                Err(err) => Err(err),
            })
        });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
