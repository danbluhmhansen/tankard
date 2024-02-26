use amqprs::channel::Channel;
use axum::{Extension, Form, Router};
use axum_extra::routing::{RouterExt, TypedPath};
use axum_htmx::HxBoosted;
use maud::{html, Markup};
use serde::Deserialize;

use crate::{
    auth::CurrentUser,
    commands::{Command, InitUser},
    components::boost,
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
        section {
            h1 { "Sign up" }
            form method="post" {
                input
                    type="text"
                    name="username"
                    placeholder="Username"
                    required;
                input
                    type="password"
                    name="password"
                    placeholder="Password"
                    required;
                button type="submit" { "Sign up" }
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
