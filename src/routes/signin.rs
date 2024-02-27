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
use sqlx::{Pool, Postgres};

use crate::{auth::CurrentUser, components::boost};

use super::profile;

async fn sign_in<'a>(
    pool: &Pool<Postgres>,
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

    if let Some(id) = user
        .id
        .zip(user.check_password)
        .filter(|(_, c)| *c)
        .map(|(id, _)| id)
    {
        let exp = Duration::from_secs(60 * 60);
        let mut claims = Claims::new_expires_in(&exp)?;
        claims.subject(&id.to_string())?;

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
        section {
            h1 { "Sign in" }
            form method="post" {
                label {
                    span { "Username" }
                    input type="text" name="username" required autofocus;
                }
                label {
                    span { "Password" }
                    input type="password" name="password" required;
                }
                button type="submit" { "Sign in" }
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
    Extension(pool): Extension<Pool<Postgres>>,
    jar: CookieJar,
    Form(Payload { username, password }): Form<Payload>,
) -> Response {
    if let Ok(Some(cookie)) = sign_in(&pool, username, password).await {
        (jar.add(cookie), Redirect::to(profile::Path.to_uri().path())).into_response()
    } else {
        boost(page(), user.is_some(), boosted).into_response()
    }
}
