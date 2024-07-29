use std::{collections::HashMap, str::FromStr};

use axum::{
    async_trait,
    body::Body,
    extract::{FromRequestParts, Path, Request, State},
    http::{request::Parts, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use axum_extra::{extract::JsonLines, TypedHeader};
use mediatype::{media_type, MediaType};
use serde::{Deserialize, Serialize};

use crate::{internal_error, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_table))
        .layer(middleware::from_fn(mdw))
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Select(pub(crate) Vec<String>);

#[async_trait]
impl<S> FromRequestParts<S> for Select {
    type Rejection = String;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        match parts.uri.query().map(Select::from_str) {
            Some(Ok(select)) => Ok(select),
            Some(Err(err)) => Err(err.to_string()),
            None => Ok(Self(vec![])),
        }
    }
}

const MT_TEXT_HTML: MediaType = media_type!(TEXT / HTML);
const MT_APPLICATION_JSON: MediaType = media_type!(APPLICATION / JSON);
const MT_TEXT_CSV: MediaType = media_type!(TEXT / CSV);

const AVAILABLE: &[MediaType] = &[MT_TEXT_HTML, MT_APPLICATION_JSON, MT_TEXT_CSV];

pub(crate) async fn get_table(
    Path(table): Path<String>,
    Select(select): Select,
    TypedHeader(accept): TypedHeader<headers_accept::Accept>,
    State(AppState { pool }): State<AppState>,
) -> Response {
    match accept.negotiate(AVAILABLE) {
        Some(mt) if mt == &MT_APPLICATION_JSON => {
            let select = if !select.is_empty() {
                &format!(
                    "jsonb_build_object({})",
                    select
                        .iter()
                        .map(|s| format!("'{s}', {s}"))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            } else {
                &table
            };
            let sql = Box::leak(Box::new(format!("select {select} from {table};")));
            let stream = sqlx::query_scalar::<_, serde_json::Value>(sql).fetch(pool);
            JsonLines::new(stream).into_response()
        }
        Some(mt) if mt == &MT_TEXT_CSV => {
            let conn = match pool.acquire().await {
                Ok(conn) => conn,
                Err(err) => return internal_error(err).into_response(),
            };

            let select = if !select.is_empty() {
                &select.join(",")
            } else {
                "*"
            };

            match Box::leak(Box::new(conn))
                .copy_out_raw(&format!(
                    "copy (select {select} from {table}) to stdout with csv header;"
                ))
                .await
            {
                Ok(stream) => Body::from_stream(stream).into_response(),
                Err(err) => internal_error(err).into_response(),
            }
        }
        Some(mt) if mt == &MT_TEXT_HTML => {
            let head = if !select.is_empty() {
                &select
                    .iter()
                    .map(|s| format!("'{s}'"))
                    .collect::<Vec<_>>()
                    .join(",")
            } else {
                let cols = sqlx::query_scalar!(
                "select column_name::text from information_schema.columns where table_name = $1;",
                &table
            )
                .fetch_all(pool)
                .await
                .map(|cols| {
                    cols.into_iter()
                        .flatten()
                        .map(|col| format!("'{col}'"))
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .map_err(internal_error);

                &match cols {
                    Ok(cols) => cols,
                    Err(err) => return err.into_response(),
                }
            };

            let select = if !select.is_empty() {
                &select
                    .iter()
                    .map(|s| format!("{s}::text"))
                    .collect::<Vec<_>>()
                    .join(",")
            } else {
                &table
            };

            // TODO: support other keys than `id`
            sqlx::query_scalar::<_, String>(&format!(
                "select html_minify(jinja_render($1, (select jsonb_build_object('head', array[{head}], 'body', (select array_agg(jsonb_build_object('key', id, 'cols', array[{select}])) from {table})))));"
            ))
            .bind(include_str!("../../tmpl/table.html"))
            .fetch_one(pool)
            .await
            .map(Html)
            .map_err(internal_error)
            .into_response()
        }
        _ => StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response(),
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Column {
    pub(crate) column_name: String,
    pub(crate) data_type: String,
}

pub(crate) async fn mdw(
    Path(table): Path<String>,
    Select(select): Select,
    request: Request,
    next: Next,
) -> Response {
    let tables = match request.extensions().get::<HashMap<String, Vec<Column>>>() {
        Some(tables) => tables,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let table = match tables.get(&table) {
        Some(table) => table,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let bad_selects: Vec<_> = select
        .iter()
        .filter(|&s| !table.iter().any(|c| &c.column_name == s))
        .map(|s| s.as_str())
        .collect();

    if !bad_selects.is_empty() {
        // TODO: better error response
        return (StatusCode::BAD_REQUEST, bad_selects.join(",")).into_response();
    }

    next.run(request).await
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use axum::{body::Body, extract::Request, http::StatusCode};
    use http_body_util::BodyExt;
    use sqlx::{Executor, PgPool};
    use tower::ServiceExt;

    use crate::app;

    #[sqlx::test]
    async fn users_html(pool: PgPool) -> Result<(), Box<dyn Error>> {
        pool.execute("create extension tankard;").await?;
        pool.execute(include_str!("../../db/sql/users.sql")).await?;
        pool.execute(include_str!("../../db/sql/html.sql")).await?;
        pool.execute("insert into users (id, username, salt, passhash, email) values ('00000000-0000-0000-0000-000000000000', 'one', '', '', 'foo'), ('00000000-0000-0000-0000-000000000001', 'two', '', '', 'foo'), ('00000000-0000-0000-0000-000000000002', 'three', '', '', 'foo');").await?;

        let app = app(Box::leak(Box::new(pool))).await?;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/users?select=username,email")
                    .header("Accept", "text/html")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
        "<table><thead><tr><th scope=col>username<th scope=col>email<tbody><tr id=00000000-0000-0000-0000-000000000000><td>one<td>foo<tr id=00000000-0000-0000-0000-000000000001><td>two<td>foo<tr id=00000000-0000-0000-0000-000000000002><td>three<td>foo</table>",
        response.into_body().collect().await?.to_bytes()
    );

        Ok(())
    }

    #[sqlx::test]
    async fn users_html_empty(pool: PgPool) -> Result<(), Box<dyn Error>> {
        pool.execute("create extension tankard;").await?;
        pool.execute(include_str!("../../db/sql/users.sql")).await?;
        pool.execute(include_str!("../../db/sql/html.sql")).await?;

        let app = app(Box::leak(Box::new(pool))).await?;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/users?select=username,email")
                    .header("Accept", "text/html")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!("", response.into_body().collect().await?.to_bytes());

        Ok(())
    }

    #[sqlx::test]
    async fn users_json(pool: PgPool) -> Result<(), Box<dyn Error>> {
        pool.execute("create extension tankard;").await?;
        pool.execute(include_str!("../../db/sql/users.sql")).await?;
        pool.execute(include_str!("../../db/sql/html.sql")).await?;
        pool.execute("insert into users (username, salt, passhash) values ('one', '', ''), ('two', '', ''), ('three', '', '');").await?;

        let app = app(Box::leak(Box::new(pool))).await?;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/users?select=username")
                    .header("Accept", "application/json")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            "{\"username\":\"one\"}\n{\"username\":\"two\"}\n{\"username\":\"three\"}\n",
            response.into_body().collect().await?.to_bytes()
        );

        Ok(())
    }

    #[sqlx::test]
    async fn users_json_empty(pool: PgPool) -> Result<(), Box<dyn Error>> {
        pool.execute("create extension tankard;").await?;
        pool.execute(include_str!("../../db/sql/users.sql")).await?;
        pool.execute(include_str!("../../db/sql/html.sql")).await?;

        let app = app(Box::leak(Box::new(pool))).await?;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/users?select=username")
                    .header("Accept", "application/json")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!("", response.into_body().collect().await?.to_bytes());

        Ok(())
    }

    // FIXME: test performance on success
    #[sqlx::test]
    async fn users_csv(pool: PgPool) -> Result<(), Box<dyn Error>> {
        pool.execute("create extension tankard;").await?;
        pool.execute(include_str!("../../db/sql/users.sql")).await?;
        pool.execute(include_str!("../../db/sql/html.sql")).await?;
        pool.execute("insert into users (username, salt, passhash, email) values ('one', '', '', 'foo'), ('two', '', '', 'foo'), ('three', '', '', 'foo');").await?;

        let app = app(Box::leak(Box::new(pool))).await?;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/users?select=username,email")
                    .header("Accept", "text/csv")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.into_body().collect().await?.to_bytes(),
            "username,email\none,foo\ntwo,foo\nthree,foo\n"
        );

        Ok(())
    }

    // FIXME: test performance on success
    #[sqlx::test]
    async fn users_csv_empty(pool: PgPool) -> Result<(), Box<dyn Error>> {
        pool.execute("create extension tankard;").await?;
        pool.execute(include_str!("../../db/sql/users.sql")).await?;
        pool.execute(include_str!("../../db/sql/html.sql")).await?;

        let app = app(Box::leak(Box::new(pool))).await?;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/users?select=username,email")
                    .header("Accept", "text/csv")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.into_body().collect().await?.to_bytes(),
            "username,email\n"
        );

        Ok(())
    }

    #[sqlx::test]
    async fn users_bad_select(pool: PgPool) -> Result<(), Box<dyn Error>> {
        pool.execute("create extension tankard;").await?;
        pool.execute(include_str!("../../db/sql/users.sql")).await?;
        pool.execute(include_str!("../../db/sql/html.sql")).await?;
        pool.execute("insert into users (username, salt, passhash) values ('one', '', ''), ('two', '', ''), ('three', '', '');").await?;

        let app = app(Box::leak(Box::new(pool))).await?;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/users?select=bad_column")
                    .header("Accept", "text/html")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.into_body().collect().await?.to_bytes(),
            "bad_column"
        );

        Ok(())
    }

    #[sqlx::test]
    async fn table_not_found(pool: PgPool) -> Result<(), Box<dyn Error>> {
        pool.execute("create extension tankard;").await?;

        let app = app(Box::leak(Box::new(pool))).await?;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/users")
                    .header("Accept", "text/html")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        Ok(())
    }
}
