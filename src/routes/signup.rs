use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Extension, Form, Router,
};
use axum_extra::routing::{RouterExt, TypedPath};
use axum_htmx::HxBoosted;
use maud::{html, Markup};
use serde::Deserialize;
use sqlx::{Pool, Postgres};

use crate::{components::boost, AppState, CurrentUser};

pub(crate) fn route() -> Router<AppState> {
    Router::new().typed_get(get).typed_post(post)
}

#[derive(TypedPath)]
#[typed_path("/signup")]
pub(crate) struct Path;

pub(crate) fn page() -> Markup {
    html! {
        form method="post" class="flex flex-col gap-2" {
            input
                type="text"
                name="username"
                placeholder="Username"
                required
                class="p-1 bg-transparent border border-black dark:border-white";
            input
                type="password"
                name="password"
                placeholder="Password"
                required
                class="p-1 bg-transparent border border-black dark:border-white";
            button type="submit" { "Sign up" }
        }
    }
}

pub(crate) async fn get(
    _: Path,
    HxBoosted(boosted): HxBoosted,
    Extension(user): Extension<Option<CurrentUser>>,
) -> Markup {
    boost(page(), user.is_some(), boosted)
}

#[derive(Deserialize)]
pub(crate) struct Payload {
    username: String,
    password: String,
}

pub(crate) async fn post(
    _: Path,
    HxBoosted(boosted): HxBoosted,
    Extension(user): Extension<Option<CurrentUser>>,
    State(state): State<Pool<Postgres>>,
    Form(Payload { username, password }): Form<Payload>,
) -> Response {
    let _ = sqlx::query!(
        "SELECT id FROM init_user(ARRAY[ROW($1, $2, gen_random_uuid())]::init_user_input[]);",
        username,
        password
    )
    .fetch_all(&state)
    .await;
    let _ = sqlx::query!("REFRESH MATERIALIZED VIEW users;")
        .fetch_all(&state)
        .await;

    boost(page(), user.is_some(), boosted).into_response()
}
