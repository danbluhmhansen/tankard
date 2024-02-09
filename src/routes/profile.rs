use axum::{
    response::{IntoResponse, Redirect, Response},
    Extension, Router,
};
use axum_extra::routing::{RouterExt, TypedPath};
use maud::html;

use crate::{components::layout, AppState, CurrentUser};

use super::index;

pub(crate) fn route() -> Router<AppState> {
    Router::new().typed_get(get)
}

#[derive(TypedPath)]
#[typed_path("/profile")]
pub(crate) struct Path;

pub(crate) async fn get(_: Path, Extension(user): Extension<Option<CurrentUser>>) -> Response {
    if let Some(user) = user {
        layout(
            html! {
                h1 { "Hello, " (user.id) "!" }
            },
            Some(user),
        )
        .into_response()
    } else {
        Redirect::to(index::Path.to_uri().path()).into_response()
    }
}
