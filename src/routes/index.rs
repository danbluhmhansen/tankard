use axum::{Extension, Router};
use axum_extra::routing::{RouterExt, TypedPath};
use axum_htmx::HxBoosted;
use maud::{html, Markup};

use crate::{components::boost, AppState, CurrentUser};

pub(crate) fn route() -> Router<AppState> {
    Router::new().typed_get(get)
}

#[derive(TypedPath)]
#[typed_path("/")]
pub(crate) struct Path;

pub(crate) fn page() -> Markup {
    html! {
        h1 { "Hello, World!" }
    }
}

pub(crate) async fn get(
    _: Path,
    HxBoosted(boosted): HxBoosted,
    Extension(user): Extension<Option<CurrentUser>>,
) -> Markup {
    boost(page(), user.is_some(), boosted)
}
