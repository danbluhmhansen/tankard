use std::{collections::HashMap, error::Error, time::Duration};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{sse::Event, Html, Sse},
    routing::get,
    Extension, Router,
};
use futures::{Stream, TryStreamExt};
use sqlx::{
    postgres::{PgListener, PgPoolOptions},
    PgPool,
};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

mod api;
mod parser;

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

#[derive(Debug, Clone)]
struct AppState {
    pool: &'static PgPool,
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
                    .and_then(|columns| serde_json::from_value::<Vec<api::Column>>(columns).ok()),
            )
        })
        .collect();

    Ok(Router::new()
        .route("/", get(index))
        .nest("/api/:table", api::router())
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
