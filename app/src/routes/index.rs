use axum::{Extension, Router};
use axum_extra::routing::{RouterExt, TypedPath};
use axum_htmx::HxBoosted;
use maud::{html, Markup};

use crate::{
    auth::CurrentUser,
    components::{boost, BTN},
};

pub(crate) fn route() -> Router {
    Router::new().typed_get(get)
}

#[derive(TypedPath)]
#[typed_path("/")]
pub(crate) struct Path;

pub(crate) fn page() -> Markup {
    html! {
        section class="flex flex-col gap-4 items-center" {
            h1 class="text-xl" { "Hello, World!" }
            button x-data "@click"="console.log('clicked')" class=(BTN) { "Click me!" }
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
