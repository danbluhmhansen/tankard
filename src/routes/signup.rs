use amqprs::channel::Channel;
use axum::{Extension, Form, Router};
use axum_extra::routing::{RouterExt, TypedPath};
use axum_htmx::HxBoosted;
use maud::{html, Markup};
use serde::Deserialize;

use crate::{
    auth::CurrentUser,
    commands::{Command, InitUser},
    components::{boost, BTN},
    Exchange, Queue,
};

pub(crate) fn route() -> Router {
    Router::new().typed_get(get).typed_post(post)
}

#[derive(TypedPath)]
#[typed_path("/signup")]
pub(crate) struct Path;

pub(crate) fn page() -> Markup {
    html! {
        section class="flex flex-col gap-4 items-center" {
            h1 class="text-xl" { "Sign up" }
            form method="post" class="flex flex-col gap-4" {
                label class="flex flex-col gap-2" {
                    span { "Username" }
                    input type="text" name="username" required autofocus class="p-1 bg-transparent rounded border";
                }
                label class="flex flex-col gap-2" {
                    span { "Password" }
                    input type="password" name="password" required class="p-1 bg-transparent rounded border";
                }
                button type="submit" class=(BTN) { "Sign up" }
            }
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
    Extension(channel): Extension<Channel>,
    Form(Payload { username, password }): Form<Payload>,
) -> Markup {
    let _ = Command::InitUser(InitUser { username, password })
        .publish(&channel, Queue::Db, Exchange::Default)
        .await;
    boost(page(), user.is_some(), boosted)
}
