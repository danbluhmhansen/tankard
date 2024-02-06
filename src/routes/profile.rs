use axum::{middleware, Extension, Router};
use axum_extra::routing::{RouterExt, TypedPath};
use maud::{html, Markup};

use crate::{auth, layout, AppState, CurrentUser};

pub(crate) fn route() -> Router<AppState> {
    Router::new()
        .typed_get(get)
        .layer(middleware::from_fn(auth))
}

#[derive(TypedPath)]
#[typed_path("/profile")]
pub(crate) struct Path;

pub(crate) async fn get(_: Path, Extension(user): Extension<CurrentUser>) -> Markup {
    layout(html! {
        h1 { "Hello, " (user.id) "!" }
    })
}
