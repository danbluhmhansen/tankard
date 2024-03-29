use std::{error::Error, time::Duration};

use axum::{
    response::{IntoResponse, Redirect, Response},
    Extension, Form, Router,
};
use axum_extra::{
    extract::{
        cookie::{Cookie, SameSite},
        CookieJar,
    },
    routing::{RouterExt, TypedPath},
};
use axum_htmx::HxBoosted;
use maud::{html, Markup};
use pasetors::{claims::Claims, keys::SymmetricKey, local, version4::V4};
use serde::Deserialize;
use sqlx::PgPool;

use crate::{
    auth::CurrentUser,
    components::{boost, BTN},
};

use super::profile;

async fn sign_in<'a>(
    pool: &PgPool,
    username: String,
    password: String,
) -> Result<Option<Cookie<'a>>, Box<dyn Error>> {
    let user = sqlx::query!(
        "SELECT id, check_password(id, $2) FROM users WHERE username = $1 LIMIT 1;",
        username,
        password
    )
    .fetch_one(pool)
    .await?;

    if user.check_password.is_some_and(|c| c) {
        let exp = Duration::from_secs(60 * 60);
        let mut claims = Claims::new_expires_in(&exp)?;
        claims.subject(&user.id.to_string())?;

        let token = local::encrypt(
            &SymmetricKey::<V4>::try_from(std::env::var("PASERK")?.as_str())?,
            &claims,
            None,
            None,
        )?;

        Ok(Some(
            Cookie::build(("session_id", token))
                .max_age(exp.try_into()?)
                .http_only(true)
                .same_site(SameSite::Strict)
                .build(),
        ))
    } else {
        Ok(None)
    }
}

pub(crate) fn route() -> Router {
    Router::new().typed_get(get).typed_post(post)
}

#[derive(TypedPath)]
#[typed_path("/signin")]
pub(crate) struct Path;

pub(crate) fn page() -> Markup {
    html! {
        section class="flex flex-col gap-4 items-center" {
            h1 class="text-xl" { "Sign in" }
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
    Extension(pool): Extension<&'static PgPool>,
    jar: CookieJar,
    Form(Payload { username, password }): Form<Payload>,
) -> Response {
    if let Ok(Some(cookie)) = sign_in(pool, username, password).await {
        (jar.add(cookie), Redirect::to(profile::Path.to_uri().path())).into_response()
    } else {
        boost(page(), user.is_some(), boosted).into_response()
    }
}
