use axum::{Extension, Router};
use axum_extra::routing::{RouterExt, TypedPath};
use axum_htmx::HxBoosted;
use maud::{html, Markup};

use crate::{auth::CurrentUser, components::boost};

pub(crate) fn route() -> Router {
    Router::new().typed_get(get)
}

#[derive(TypedPath)]
#[typed_path("/")]
pub(crate) struct Path;

pub(crate) fn page() -> Markup {
    html! {
        section {
            h1 { "Hello, World!" }
            button "@click"="console.log('clicked')" { "Click me!" }
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
