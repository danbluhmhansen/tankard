use axum::{
    extract::Request,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    Extension,
};
use axum_extra::extract::CookieJar;
use pasetors::{
    claims::ClaimsValidationRules, keys::SymmetricKey, local, token::UntrustedToken, version4::V4,
    Local,
};
use uuid::Uuid;

use crate::routes::index;

#[derive(Clone, Debug)]
pub(crate) struct CurrentUser {
    pub(crate) id: Uuid,
}

pub(crate) async fn auth(jar: CookieJar, mut req: Request, next: Next) -> Response {
    if let Some(id) = jar
        .get("session_id")
        .and_then(|c| UntrustedToken::<Local, V4>::try_from(c.value()).ok())
        .zip(
            std::env::var("PASERK")
                .ok()
                .and_then(|k| SymmetricKey::<V4>::try_from(k.as_str()).ok()),
        )
        .and_then(|(token, key)| {
            local::decrypt(&key, &token, &ClaimsValidationRules::new(), None, None).ok()
        })
        .and_then(|token| {
            token
                .payload_claims()
                .and_then(|c| c.get_claim("sub"))
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
        })
    {
        req.extensions_mut().insert(Some(CurrentUser { id }));
    } else {
        req.extensions_mut().insert(None::<CurrentUser>);
    }
    next.run(req).await
}

pub(crate) async fn req_auth(
    Extension(user): Extension<Option<CurrentUser>>,
    mut req: Request,
    next: Next,
) -> Response {
    if let Some(user) = user {
        req.extensions_mut().remove::<Option<CurrentUser>>();
        req.extensions_mut().insert(user);
        next.run(req).await
    } else {
        Redirect::to(&index::Path.to_string()).into_response()
    }
}
