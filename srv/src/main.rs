use std::{collections::HashMap, error::Error, time::Duration};

use axum::{
    async_trait,
    body::Body,
    extract::{FromRequestParts, Path, Request, State},
    http::{request::Parts, StatusCode},
    middleware::{self, Next},
    response::{sse::Event, Html, IntoResponse, Response, Sse},
    routing::get,
    Extension, Json, Router,
};
use axum_extra::TypedHeader;
use futures::{Stream, TryStreamExt};
use serde::{Deserialize, Serialize};
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
    State(AppState { pool }): State<AppState>,
) -> Result<Html<String>, (StatusCode, String)> {
    sqlx::query_scalar("select html_minify(html_index());")
        .fetch_one(pool)
        .await
        .map(Html)
        .map_err(internal_error)
}

async fn api_table(
    Path(table): Path<String>,
    ApiQuery { select }: ApiQuery,
    TypedHeader(accept): TypedHeader<headers_accept::Accept>,
    State(AppState { pool }): State<AppState>,
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
            &table
        };
        sqlx::query_scalar::<_, serde_json::Value>(&format!(
            "select coalesce(jsonb_agg({select}), 'null'::jsonb) from {table};"
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
            let cols = sqlx::query_scalar::<_, String>(
                "select column_name::text from information_schema.columns where table_name = $1;",
            )
            .bind(&table)
            .fetch_all(pool)
            .await
            .map(|cols| {
                cols.into_iter()
                    .map(|col| format!("'{col}'"))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .map_err(internal_error);
            if let Err(err) = cols {
                return err.into_response();
            }
            &cols.unwrap_or_default()
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
}

async fn listen(
    Path(event): Path<String>,
    State(AppState { pool }): State<AppState>,
) -> Result<Sse<impl Stream<Item = Result<Event, sqlx::Error>>>, (StatusCode, String)> {
    match PgListener::connect_with(pool).await {
        Ok(mut listener) => {
            _ = listener.listen(&event).await;
            Ok(Sse::new(listener.into_stream().map_ok(move |n| {
                Event::default().event(&event).data(n.payload())
            })))
        }
        Err(err) => Err(internal_error(err)),
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Column {
    column_name: String,
    data_type: String,
}

#[derive(Debug, Clone)]
struct AppState {
    pool: &'static PgPool,
}

async fn api_mdw(
    Path(table): Path<String>,
    ApiQuery { select }: ApiQuery,
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

async fn app(pool: &'static PgPool) -> Result<Router, sqlx::Error> {
    // TODO: live refresh of schema
    let tables: HashMap<_, _> = sqlx::query_file!("sql/schema_tables_columns.sql")
        .fetch_all(pool)
        .await?
        .into_iter()
        .filter_map(|row| {
            row.table_name.zip(
                row.columns
                    .and_then(|columns| serde_json::from_value::<Vec<Column>>(columns).ok()),
            )
        })
        .collect();

    Ok(Router::new()
        .route("/", get(index))
        .nest(
            "/api/:table",
            Router::new()
                .route("/", get(api_table))
                .layer(middleware::from_fn(api_mdw)),
        )
        .route("/listen/:event", get(listen))
        .fallback_service(ServeDir::new("dist"))
        .layer(Extension(tables))
        .with_state(AppState { pool }))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let db_url = std::option_env!("DATABASE_URL").unwrap_or("postgres://localhost:28816/tankard");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(db_url)
        .await?;

    let app = app(Box::leak(Box::new(pool))).await?;

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
        pool.execute("insert into users (id, username, salt, passhash, email) values ('00000000-0000-0000-0000-000000000000', 'one', '', '', 'foo'), ('00000000-0000-0000-0000-000000000001', 'two', '', '', 'foo'), ('00000000-0000-0000-0000-000000000002', 'three', '', '', 'foo');").await?;

        let app = app(Box::leak(Box::new(pool))).await?;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/users?select=username,email")
                    .header("Accept", "*/*")
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
                    .header("Accept", "*/*")
                    .body(Body::empty())?,
            )
            .await?;

        // assert_eq!(response.status(), StatusCode::OK);

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
                    .header("Accept", "*/*")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

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
                    .header("Accept", "*/*")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

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
            .await?
            .oneshot(
                Request::builder()
                    .uri("/listen/users_event")
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
