use std::{error::Error, time::Duration};

use axum::{
    async_trait,
    body::Body,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{sse::Event, Html, IntoResponse, Response, Sse},
    routing::get,
    Extension, Json, Router,
};
use axum_extra::TypedHeader;
use futures::{Stream, TryStreamExt};
use sqlx::{
    postgres::{PgListener, PgPoolOptions},
    PgPool,
};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

mod parser;

#[derive(Debug)]
struct ApiQuery {
    select: Vec<String>,
}

#[async_trait]
impl<S> FromRequestParts<S> for ApiQuery {
    type Rejection = String;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        match parts.uri.query().map(|query| parser::query_select(query)) {
            Some(Ok((_, select))) => Ok(Self {
                select: select.into_iter().map(|s| s.to_string()).collect(),
            }),
            Some(Err(err)) => Err(err.to_string()),
            None => Ok(Self { select: vec![] }),
        }
    }
}

fn internal_error<E>(err: E) -> (StatusCode, String)
where
    E: std::error::Error,
{
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}

async fn index(
    Extension(pool): Extension<&'static PgPool>,
) -> Result<Html<String>, (StatusCode, String)> {
    sqlx::query_scalar("select html_minify(html_index());")
        .fetch_one(pool)
        .await
        .map(Html)
        .map_err(internal_error)
}

async fn users(
    ApiQuery { select }: ApiQuery,
    TypedHeader(accept): TypedHeader<headers_accept::Accept>,
    Extension(pool): Extension<&'static PgPool>,
) -> Response {
    if accept
        .media_types()
        .any(|mt| mt.to_string() == "application/json")
    {
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
            "users"
        };
        // FIXME: sql injection?
        sqlx::query_scalar::<_, serde_json::Value>(&format!(
            "select coalesce(jsonb_agg({select}), 'null'::jsonb) from users;"
        ))
        .fetch_one(pool)
        .await
        .map(Json)
        .map_err(internal_error)
        .into_response()
    } else if accept.media_types().any(|mt| mt.to_string() == "text/csv") {
        match pool.acquire().await {
            Ok(conn) => {
                let select = if !select.is_empty() {
                    &select.join(",")
                } else {
                    "*"
                };
                // FIXME: sql injection?
                match Box::leak(Box::new(conn))
                    .copy_out_raw(&format!(
                        "copy (select {select} from users) to stdout with csv header;"
                    ))
                    .await
                {
                    Ok(stream) => Body::from_stream(stream).into_response(),
                    Err(err) => internal_error(err).into_response(),
                }
            }
            Err(err) => internal_error(err).into_response(),
        }
    } else {
        let head = if !select.is_empty() {
            &select
                .iter()
                .map(|s| format!("'{s}'"))
                .collect::<Vec<_>>()
                .join(",")
        } else {
            "'id','added','updated','username','salt','passhash','email'"
        };
        let select = if !select.is_empty() {
            &select
                .iter()
                .map(|s| format!("{s}::text"))
                .collect::<Vec<_>>()
                .join(",")
        } else {
            "users"
        };
        // FIXME: sql injection?
        sqlx::query_scalar::<_, String>(&format!(
            "select html_minify(array_to_html(array[{head}], (select array_agg(array[{select}]) from users)));"
        ))
        .fetch_one(pool)
        .await
        .map(Html)
        .map_err(internal_error)
        .into_response()
    }
}

async fn users_listen(
    Extension(pool): Extension<&'static PgPool>,
) -> Result<Sse<impl Stream<Item = Result<Event, sqlx::Error>>>, (StatusCode, String)> {
    match PgListener::connect_with(pool).await {
        Ok(mut listener) => {
            _ = listener.listen("users_event").await;
            Ok(Sse::new(listener.into_stream().map_ok(|n| {
                Event::default().event("users_event").data(n.payload())
            })))
        }
        Err(err) => Err(internal_error(err)),
    }
}

fn app(pool: &'static PgPool) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/users", get(users))
        .route("/users_listen", get(users_listen))
        .fallback_service(ServeDir::new("dist"))
        .layer(Extension(pool))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let db_url = std::option_env!("DATABASE_URL").unwrap_or("postgres://localhost:28816/tankard");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(db_url)
        .await?;

    let app = app(Box::leak(Box::new(pool)));

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
    use futures::StreamExt;
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

        let app = app(Box::leak(Box::new(pool)));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/users?select=username,email")
                    .header("Accept", "*/*")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);

        assert_eq!(
            "<table><thead><tr><th scope=col>username<th scope=col>email<tbody><tr><td>one<td>foo<tr><td>two<td>foo<tr><td>three<td>foo</table>",
            response.into_body().collect().await?.to_bytes()
        );

        Ok(())
    }

    #[sqlx::test]
    async fn users_html_empty(pool: PgPool) -> Result<(), Box<dyn Error>> {
        pool.execute("create extension tankard;").await?;
        pool.execute(include_str!("../../db/sql/users.sql")).await?;
        pool.execute(include_str!("../../db/sql/html.sql")).await?;

        let app = app(Box::leak(Box::new(pool)));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/users?select=username,email")
                    .header("Accept", "*/*")
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

        let app = app(Box::leak(Box::new(pool)));

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

    #[sqlx::test]
    async fn users_json_empty(pool: PgPool) -> Result<(), Box<dyn Error>> {
        pool.execute("create extension tankard;").await?;
        pool.execute(include_str!("../../db/sql/users.sql")).await?;
        pool.execute(include_str!("../../db/sql/html.sql")).await?;

        let app = app(Box::leak(Box::new(pool)));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/users?select=username")
                    .header("Accept", "application/json")
                    .body(Body::empty())?,
            )
            .await?;

        // assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await?.to_bytes();

        assert_eq!(
            serde_json::Value::Null,
            serde_json::from_slice::<serde_json::Value>(&body)?
        );

        Ok(())
    }

    // FIXME: test performance on success
    #[sqlx::test]
    async fn users_csv(pool: PgPool) -> Result<(), Box<dyn Error>> {
        pool.execute("create extension tankard;").await?;
        pool.execute(include_str!("../../db/sql/users.sql")).await?;
        pool.execute(include_str!("../../db/sql/html.sql")).await?;
        pool.execute("insert into users (username, salt, passhash, email) values ('one', '', '', 'foo'), ('two', '', '', 'foo'), ('three', '', '', 'foo');").await?;

        let app = app(Box::leak(Box::new(pool)));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/users?select=username,email")
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

        let app = app(Box::leak(Box::new(pool)));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/users?select=username,email")
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
    async fn users_listen(pool: PgPool) -> Result<(), Box<dyn Error>> {
        let pool = Box::leak(Box::new(pool));
        pool.execute("create extension tankard;").await?;
        pool.execute(include_str!("../../db/sql/users.sql")).await?;
        pool.execute(include_str!("../../db/sql/html.sql")).await?;
        pool.execute("insert into users (username, salt, passhash) values ('one', '', ''), ('two', '', ''), ('three', '', '');")
            .await?;

        let response = app(pool)
            .oneshot(
                Request::builder()
                    .uri("/users_listen")
                    .header("Accept", "*/*")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);

        pool.execute("insert into users (username, salt, passhash) values ('four', '', '');")
            .await?;

        assert_eq!(
            response.into_data_stream().next().await.unwrap()?,
            "event: users_event\ndata: \n\n"
        );

        Ok(())
    }
}
