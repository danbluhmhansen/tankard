use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Form, Router,
};
use axum_extra::{
    extract::CookieJar,
    routing::{RouterExt, TypedPath},
};
use maud::{html, Markup};
use serde::Deserialize;
use sqlx::{Pool, Postgres};

use crate::{layout, sign_in, AppState};

pub(crate) fn route() -> Router<AppState> {
    Router::new().typed_get(get).typed_post(post)
}

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

pub(crate) async fn get(_: Path) -> Markup {
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
            if let Ok(Some(cookie)) = sign_in(&state, form.username, form.password).await {
                (jar.add(cookie), page()).into_response()
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
