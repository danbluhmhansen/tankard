use axum::{
    extract::State,
    response::{IntoResponse, Redirect, Response},
    Extension, Router,
};
use axum_extra::routing::{RouterExt, TypedPath};
use axum_htmx::HxBoosted;
use maud::{html, Markup};
use sqlx::{Pool, Postgres};

use crate::{components::boost, AppState, CurrentUser};

use super::index;

pub(crate) fn route() -> Router<AppState> {
    Router::new().typed_get(get)
}

pub(crate) fn page(username: String) -> Markup {
    html! {
        h1 { "Hello, " (username) "!" }
    }
}

#[derive(TypedPath)]
#[typed_path("/profile")]
pub(crate) struct Path;

pub(crate) async fn get(
    _: Path,
    HxBoosted(boosted): HxBoosted,
    Extension(user): Extension<Option<CurrentUser>>,
    State(state): State<Pool<Postgres>>,
) -> Response {
    if let Some(user) = user {
        let user = sqlx::query!("SELECT username FROM users WHERE id = $1 LIMIT 1;", user.id)
            .fetch_one(&state)
            .await
            .unwrap();
        boost(page(user.username.unwrap()), true, boosted).into_response()
    } else {
        Redirect::to(index::Path.to_uri().path()).into_response()
    }
}
