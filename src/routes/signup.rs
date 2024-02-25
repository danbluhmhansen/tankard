use amqprs::{
    channel::{BasicPublishArguments, Channel},
    BasicProperties,
};
use axum::{Extension, Form, Router};
use axum_extra::routing::{RouterExt, TypedPath};
use axum_htmx::HxBoosted;
use maud::{html, Markup};
use serde::Deserialize;

use crate::{auth::CurrentUser, commands::Command, components::boost, Queue};

pub(crate) fn route() -> Router {
    Router::new().typed_get(get).typed_post(post)
}

#[derive(TypedPath)]
#[typed_path("/signup")]
pub(crate) struct Path;

pub(crate) fn page() -> Markup {
    html! {
        form method="post" class="flex flex-col gap-2" {
            input
                type="text"
                name="username"
                placeholder="Username"
                required
                class="p-1 bg-transparent border border-black dark:border-white";
            input
                type="password"
                name="password"
                placeholder="Password"
                required
                class="p-1 bg-transparent border border-black dark:border-white";
            button type="submit" { "Sign up" }
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
    if let Ok(content) = (Command::InitUser { username, password }).try_into() {
        let _ = channel
            .basic_publish(
                BasicProperties::default(),
                content,
                BasicPublishArguments::new("", Queue::Db.into()),
            )
            .await;
    }
    boost(page(), user.is_some(), boosted)
}
