use axum::{
    response::{IntoResponse, Redirect, Response},
    Extension, Router,
};
use axum_extra::routing::{RouterExt, TypedPath};
use axum_htmx::HxBoosted;
use maud::{html, Markup};

use crate::{components::boost, AppState, CurrentUser};

use super::index;

pub(crate) fn route() -> Router<AppState> {
    Router::new().typed_get(get)
}

pub(crate) fn page(user: CurrentUser) -> Markup {
    html! {
        h1 { "Hello, " (user.id) "!" }
    }
}

#[derive(TypedPath)]
#[typed_path("/profile")]
pub(crate) struct Path;

pub(crate) async fn get(
    _: Path,
    HxBoosted(boosted): HxBoosted,
    Extension(user): Extension<Option<CurrentUser>>,
) -> Response {
    if let Some(user) = user {
        boost(page(user), true, boosted).into_response()
    } else {
        Redirect::to(index::Path.to_uri().path()).into_response()
    }
}
