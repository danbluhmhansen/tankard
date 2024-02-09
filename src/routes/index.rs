use axum::{Extension, Router};
use axum_extra::routing::{RouterExt, TypedPath};
use maud::{html, Markup};

use crate::{components::layout, AppState, CurrentUser};

pub(crate) fn route() -> Router<AppState> {
    Router::new().typed_get(get)
}

#[derive(TypedPath)]
#[typed_path("/")]
pub(crate) struct Path;

pub(crate) fn page(user: Option<CurrentUser>) -> Markup {
    layout(
        html! {
            h1 { "Hello, World!" }
        },
        user,
    )
}

pub(crate) async fn get(_: Path, Extension(user): Extension<Option<CurrentUser>>) -> Markup {
    page(user)
}
