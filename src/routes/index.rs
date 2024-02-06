use std::time::Duration;

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Extension, Form,
};
use axum_extra::{
    extract::{
        cookie::{Cookie, SameSite},
        CookieJar,
    },
    routing::TypedPath,
};
use maud::{html, Markup};
use pasetors::{claims::Claims, keys::SymmetricKey, local, version4::V4};
use serde::Deserialize;
use sqlx::{Pool, Postgres};

use crate::{layout, CurrentUser};

#[derive(TypedPath)]
#[typed_path("/")]
pub(crate) struct Path;

pub(crate) fn page() -> Markup {
    layout(html! {
        form method="post" {
            input type="text" name="username" placeholder="Username" required class="bg-transparent";
            input type="password" name="password" placeholder="Password" required class="bg-transparent";
            button type="submit" name="submit" value="signin" { "Sign in" }
        }
        form method="post" {
            input type="text" name="username" placeholder="Username" required class="bg-transparent";
            input type="password" name="password" placeholder="Password" required class="bg-transparent";
            button type="submit" name="submit" value="signup" { "Sign up" }
        }
    })
}

pub(crate) async fn get(
    _: Path,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Markup {
    println!("{current_user:?}");
    page()
}

#[derive(Deserialize)]
pub(crate) struct Payload {
    username: String,
    password: String,
    submit: String,
}

pub(crate) async fn post(
    _: Path,
    State(state): State<Pool<Postgres>>,
    jar: CookieJar,
    Form(form): Form<Payload>,
) -> Response {
    match form.submit.as_str() {
        "signin" => {
            let user = sqlx::query!("SELECT id FROM users WHERE username = $1;", form.username)
                .fetch_one(&state)
                .await
                .unwrap();

            let success =
                sqlx::query_scalar!("SELECT check_password($1, $2);", user.id, form.password)
                    .fetch_one(&state)
                    .await
                    .unwrap();

            if success.is_some_and(|s| s) {
                let mut claims = Claims::new_expires_in(&Duration::from_secs(120)).unwrap();
                claims
                    .subject(user.id.unwrap().to_string().as_str())
                    .unwrap();

                let token = local::encrypt(
                    &SymmetricKey::<V4>::try_from(std::env::var("PASERK").unwrap().as_str())
                        .unwrap(),
                    &claims,
                    None,
                    None,
                )
                .unwrap();

                (
                    jar.add(
                        Cookie::build(("session_id", token))
                            .http_only(true)
                            .same_site(SameSite::Strict)
                            .build(),
                    ),
                    page(),
                )
                    .into_response()
            } else {
                page().into_response()
            }
        }
        "signup" => {
            let _ = sqlx::query!("SELECT init_user($1, $2);", form.username, form.password)
                .fetch_all(&state)
                .await;
            let _ = sqlx::query!("REFRESH MATERIALIZED VIEW users;")
                .fetch_all(&state)
                .await;

            page().into_response()
        }
        _ => unreachable!(),
    }
}
