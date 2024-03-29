use axum::{
    response::{IntoResponse, Redirect, Response},
    Router,
};
use axum_extra::{
    extract::CookieJar,
    routing::{RouterExt, TypedPath},
};

use super::index;

pub(crate) fn route() -> Router {
    Router::new().typed_post(post)
}

#[derive(TypedPath)]
#[typed_path("/signout")]
pub(crate) struct Path;

pub(crate) async fn post(_: Path, jar: CookieJar) -> Response {
    if let Some(cookie) = jar.get("session_id").cloned() {
        (jar.remove(cookie), Redirect::to(&index::Path.to_string())).into_response()
    } else {
        Redirect::to(&index::Path.to_string()).into_response()
    }
}
