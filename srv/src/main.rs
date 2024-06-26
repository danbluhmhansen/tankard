use std::{error::Error, time::Duration};

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use axum_extra::{extract::Query, TypedHeader};
use serde::Deserialize;
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::net::TcpListener;

#[derive(Debug, Deserialize)]
struct ApiQuery {
    select: Option<String>,
}

fn internal_error<E>(err: E) -> (StatusCode, String)
where
    E: std::error::Error,
{
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}

async fn index(State(pool): State<PgPool>) -> Result<Html<String>, (StatusCode, String)> {
    sqlx::query_scalar("select html_minify(html_index());")
        .fetch_one(&pool)
        .await
        .map(Html)
        .map_err(internal_error)
}

async fn users(
    Query(query): Query<ApiQuery>,
    accept: Option<TypedHeader<headers_accept::Accept>>,
    State(pool): State<PgPool>,
) -> Response {
    match accept {
        Some(TypedHeader(accept))
            if accept
                .media_types()
                .any(|mt| mt.essence().to_string() == "application/json") =>
        {
            if let Some(select) = query.select.map(|select| {
                select
                    .split(',')
                    .map(|s| format!("'{s}', {s}"))
                    .collect::<Vec<_>>()
                    .join(",")
            }) {
                // FIXME: sql injection?
                sqlx::query_scalar::<_, serde_json::Value>(&format!(
                    "select jsonb_agg(jsonb_build_object({select})) from users;"
                ))
                .fetch_one(&pool)
                .await
                .map(Json)
                .map_err(internal_error)
                .into_response()
            } else {
                sqlx::query_scalar!("select jsonb_agg(users) from users;")
                    .fetch_one(&pool)
                    .await
                    .map(Json)
                    .map_err(internal_error)
                    .into_response()
            }
        }
        // TODO: read column selection from ApiQuery
        _ => sqlx::query_scalar!(
            "select array_to_html(array['Username', 'Email'], (select array_agg(array[username, email]) from users));"
        )
        .fetch_one(&pool)
        .await
        .map(|html| Html(html.unwrap_or("".to_string())))
        .map_err(internal_error)
        .into_response(),
    }
}

fn app(pool: PgPool) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/users", get(users))
        .with_state(pool)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let db_url = std::option_env!("DATABASE_URL").unwrap_or("postgres://localhost:28816/tankard");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(db_url)
        .await?;

    let app = app(pool);

    let listener = TcpListener::bind("127.0.0.1:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use sqlx::{Executor, PgPool};
    use tower::ServiceExt;

    use crate::app;

    #[sqlx::test]
    async fn users_html(pool: PgPool) -> Result<(), Box<dyn Error>> {
        pool.execute("create extension tankard;").await?;
        pool.execute(include_str!("../../db/sql/users.sql")).await?;
        pool.execute(include_str!("../../db/sql/html.sql")).await?;
        pool.execute("insert into users (username, salt, passhash, email) values ('one', '', '', 'foo'), ('two', '', '', 'foo'), ('three', '', '', 'foo');").await?;

        let app = app(pool);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/users?select=username,email")
                    .header("Accept", "*/*")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await?.to_bytes();

        assert_eq!(
            "<table><thead><tr><th scope=col>Username</th><th scope=col>Email</th></tr></thead><tbody><tr><td>one</td><td>foo</td></tr><tr><td>two</td><td>foo</td></tr><tr><td>three</td><td>foo</td></tr></tbody></table>",
            String::from_utf8(body.into())?
        );

        Ok(())
    }

    #[sqlx::test]
    async fn users_json(pool: PgPool) -> Result<(), Box<dyn Error>> {
        pool.execute("create extension tankard;").await?;
        pool.execute(include_str!("../../db/sql/users.sql")).await?;
        pool.execute(include_str!("../../db/sql/html.sql")).await?;
        pool.execute("insert into users (username, salt, passhash) values ('one', '', ''), ('two', '', ''), ('three', '', '');").await?;

        let app = app(pool);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/users?select=username")
                    .header("Accept", "application/json")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await?.to_bytes();

        assert_eq!(
            serde_json::json!([{ "username": "one" }, { "username": "two" }, { "username": "three" }]),
            serde_json::from_slice::<serde_json::Value>(&body)?
        );

        Ok(())
    }
}
