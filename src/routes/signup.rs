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
        form method="post" class="flex gap-2" {
            input
                type="text"
                name="username"
                placeholder="Username"
                required
                class="bg-transparent p-1 border border-black dark:border-white";
            input
                type="password"
                name="password"
                placeholder="Password"
                required
                class="bg-transparent p-1 border border-black dark:border-white";
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
    Form(form): Form<Payload>,
) -> Response {
    let _ = sqlx::query!("SELECT init_user($1, $2);", form.username, form.password)
        .fetch_all(&state)
        .await;
    let _ = sqlx::query!("REFRESH MATERIALIZED VIEW users;")
        .fetch_all(&state)
        .await;

    boost(page(), user.is_some(), boosted).into_response()
}