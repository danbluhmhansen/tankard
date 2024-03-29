use axum::{
    middleware,
    response::{IntoResponse, Response},
    Extension, Router,
};
use axum_extra::routing::{RouterExt, TypedPath};
use axum_htmx::HxBoosted;
use futures_util::TryFutureExt;
use maud::{html, Markup};
use sqlx::PgPool;

use crate::{
    auth::{self, CurrentUser},
    components::boost,
};

pub(crate) fn route() -> Router {
    Router::new()
        .typed_get(get)
        .layer(middleware::from_fn(auth::req_auth))
}

pub(crate) fn page(username: String) -> Markup {
    html! {
        section class="flex flex-col gap-4 items-center" { h1 class="text-xl" { "Hello, " (username) "!" } }
    }
}

#[derive(TypedPath)]
#[typed_path("/profile")]
pub(crate) struct Path;

pub(crate) async fn get(
    _: Path,
    HxBoosted(boosted): HxBoosted,
    Extension(CurrentUser { id }): Extension<CurrentUser>,
    Extension(pool): Extension<&'static PgPool>,
) -> Response {
    if let Ok(username) = sqlx::query!("SELECT username FROM users WHERE id = $1 LIMIT 1;", id)
        .fetch_one(pool)
        .map_ok(|user| user.username)
        .await
    {
        boost(page(username), true, boosted).into_response()
    } else {
        todo!()
    }
}
