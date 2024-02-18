use std::sync::Arc;

use axum::{
    response::{IntoResponse, Response},
    Extension, Form, Router,
};
use axum_extra::routing::{RouterExt, TypedPath};
use axum_htmx::HxBoosted;
use lapin::{options::BasicPublishOptions, BasicProperties, Channel};
use maud::{html, Markup};
use serde::{Deserialize, Serialize};

use crate::{components::boost, AppState, CurrentUser};

pub(crate) fn route() -> Router<Arc<AppState>> {
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

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct InitUser {
    pub(crate) username: String,
    pub(crate) password: String,
}

impl InitUser {
    pub(crate) fn new(username: String, password: String) -> Self {
        Self { username, password }
    }
}

pub(crate) async fn post(
    _: Path,
    HxBoosted(boosted): HxBoosted,
    Extension(user): Extension<Option<CurrentUser>>,
    Extension(channel): Extension<Channel>,
    Form(Payload { username, password }): Form<Payload>,
) -> Response {
    if let Ok(init_user) = serde_json::to_vec(&InitUser::new(username, password)) {
        let _ = channel
            .basic_publish(
                "",
                "db",
                BasicPublishOptions::default(),
                &init_user,
                BasicProperties::default(),
            )
            .await;
    }
    boost(page(), user.is_some(), boosted).into_response()
}
