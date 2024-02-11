use axum::{
    extract::State,
    response::{IntoResponse, Redirect, Response},
    Extension, Form, Router,
};
use axum_extra::routing::{RouterExt, TypedPath};
use axum_htmx::HxBoosted;
use maud::{html, Markup};
use serde::Deserialize;
use sqlx::{types::Uuid, Pool, Postgres};

use crate::{components::boost, AppState, CurrentUser};

use super::index;

pub(crate) fn route() -> Router<AppState> {
    Router::new().typed_get(get).typed_post(post)
}

pub(crate) async fn page(user_id: Uuid, pool: &Pool<Postgres>) -> Markup {
    let games = sqlx::query!("SELECT name FROM games WHERE user_id = $1;", user_id)
        .fetch_all(pool)
        .await;
    html! {
        @if let Ok(games) = games {
            table {
                thead { tr { th { "Name" } } }
                tbody {
                    @for game in games { tr { td { @if let Some(name) = game.name { (name) } } } }
                }
            }
        } @else {
            p { "No games..." }
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
#[typed_path("/games")]
pub(crate) struct Path;

pub(crate) async fn get(
    _: Path,
    HxBoosted(boosted): HxBoosted,
    Extension(user): Extension<Option<CurrentUser>>,
    State(state): State<Pool<Postgres>>,
) -> Response {
    if let Some(CurrentUser { id }) = user {
        boost(page(id, &state).await, true, boosted).into_response()
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
    HxBoosted(boosted): HxBoosted,
    Extension(user): Extension<Option<CurrentUser>>,
    State(state): State<Pool<Postgres>>,
    Form(Payload { name, description }): Form<Payload>,
) -> Response {
    if let Some(CurrentUser { id }) = user {
        let _ = sqlx::query!(
            "SELECT id FROM init_games(ARRAY[ROW($1, $2, $3, gen_random_uuid())]::init_games_input[]);",
            id,
            name,
            description
        )
        .fetch_all(&state)
        .await;
        let _ = sqlx::query!("REFRESH MATERIALIZED VIEW games;")
            .fetch_all(&state)
            .await;

        boost(page(id, &state).await, true, boosted).into_response()
    } else {
        Redirect::to(index::Path.to_uri().path()).into_response()
    }
}
