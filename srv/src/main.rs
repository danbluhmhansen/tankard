use std::{collections::HashMap, error::Error};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{sse::Event, Html, Sse},
    routing::get,
    Extension, Router,
};
use bb8_postgres::PostgresConnectionManager;
use futures::{channel::mpsc, Stream, StreamExt};
use tokio::net::TcpListener;
use tokio_postgres::{AsyncMessage, NoTls};
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
    State(AppState { pool, .. }): State<AppState>,
) -> Result<Html<String>, (StatusCode, String)> {
    let conn = pool.get().await.unwrap();
    conn.query_one("select html_minify(html_index());", &[])
        .await
        .map(|row| Html(row.get(0)))
        .map_err(internal_error)
}

// FIXME: no notifications are received
async fn listen(
    Path(event): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, String>>>, (StatusCode, String)> {
    println!("trying to listen to event: {event}");
    let db_url = std::option_env!("DATABASE_URL").unwrap_or("postgres://localhost:28816/tankard");
    let (client, _) = tokio_postgres::connect(db_url, NoTls).await.unwrap();

    let (_, rx) = mpsc::unbounded();
    client
        .execute(&format!("listen {event};"), &[])
        .await
        .unwrap();

    // drop(client);

    let notifications = rx.map(move |m| match m {
        AsyncMessage::Notification(n) => {
            println!("notification: {n:?}");
            Ok(Event::default().event(&event).data(n.payload()))
        }
        AsyncMessage::Notice(n) => {
            println!("notice: {n}");
            Ok(Event::default())
        }
        _ => {
            println!("discard message");
            Ok(Event::default())
        }
    });

    Ok(Sse::new(notifications))
}

#[derive(Debug, Clone)]
struct AppState {
    pool: &'static bb8::Pool<PostgresConnectionManager<NoTls>>,
}

async fn app(
    pool: &'static bb8::Pool<PostgresConnectionManager<NoTls>>,
) -> Result<Router, Box<dyn Error>> {
    // TODO: live refresh of schema
    let conn = pool.get().await?;
    let tables = conn
        .query(include_str!("../sql/schema_tables_columns.sql"), &[])
        .await
        .map(|rows| {
            rows.into_iter()
                .filter_map(|row| {
                    serde_json::from_value::<Vec<api::Column>>(
                        row.get::<_, serde_json::Value>("columns"),
                    )
                    .ok()
                    .map(|columns| (row.get::<_, String>("table_name"), columns))
                })
                .collect::<HashMap<_, _>>()
        })?;

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

    let manager = PostgresConnectionManager::new_from_stringlike(db_url, NoTls)?;
    let pool = bb8::Pool::builder().build(manager).await?;

    let app = app(Box::leak(Box::new(pool))).await?;

    let listener = TcpListener::bind("127.0.0.1:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// TODO: fix sse and re-enable tests for it
// #[cfg(test)]
// mod tests {
//     use std::error::Error;

//     use axum::{
//         body::Body,
//         http::{Request, StatusCode},
//     };
//     use bb8_postgres::PostgresConnectionManager;
//     use futures::StreamExt;
//     use http_body_util::BodyExt;
//     use sqlx::{Executor, PgPool};
//     use tokio_postgres::NoTls;
//     use tower::ServiceExt;

//     use crate::app;

//     #[sqlx::test]
//     async fn users_listen(pool: PgPool) -> Result<(), Box<dyn Error>> {
//         let conn_options = pool.connect_options();
//         let db_name = conn_options.get_database().unwrap_or("");
//         let manager = PostgresConnectionManager::new_from_stringlike(
//             format!("postgres://localhost:28817/{db_name}"),
//             NoTls,
//         )?;
//         let pool2 = Box::leak(Box::new(bb8::Pool::builder().build(manager).await?));
//         let pool = Box::leak(Box::new(pool));
//         pool.execute("create extension tankard;").await?;
//         pool.execute(include_str!("../../db/sql/users.sql")).await?;
//         pool.execute(include_str!("../../db/sql/html.sql")).await?;
//         pool.execute("insert into users (username, salt, passhash) values ('one', '', ''), ('two', '', ''), ('three', '', '');")
//             .await?;

//         let response = app(pool, pool2)
//             .await?
//             .oneshot(
//                 Request::builder()
//                     .uri("/listen/users_event")
//                     .header("Accept", "*/*")
//                     .body(Body::empty())?,
//             )
//             .await?;

//         assert_eq!(response.status(), StatusCode::OK);

//         pool.execute("insert into users (username, salt, passhash) values ('four', '', '');")
//             .await?;

//         assert_eq!(
//             response.into_data_stream().next().await.unwrap()?,
//             "event: users_event\ndata: \n\n"
//         );

//         Ok(())
//     }
// }
