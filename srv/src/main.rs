use std::{collections::HashMap, error::Error};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{sse::Event, Html, Sse},
    routing::get,
    Extension, Router,
};
use bb8_postgres::PostgresConnectionManager;
use futures::{channel::mpsc, FutureExt, Stream, StreamExt, TryFutureExt, TryStreamExt};
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
    let conn = match pool.get().await.map_err(internal_error) {
        Ok(conn) => conn,
        Err(err) => return Err(err),
    };
    conn.query_one("select html_minify(html_index());", &[])
        .await
        .map(|row| Html(row.get(0)))
        .map_err(internal_error)
}

async fn listen(
    Path(event): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, String>>>, (StatusCode, String)> {
    let db_url = std::option_env!("DATABASE_URL").unwrap_or("postgres://localhost:28816/tankard");
    let (client, mut conn) = match tokio_postgres::connect(db_url, NoTls)
        .map_err(internal_error)
        .await
    {
        Ok(conn) => conn,
        Err(err) => return Err(err),
    };

    let (tx, rx) = mpsc::unbounded();

    let stream =
        futures::stream::poll_fn(move |cx| conn.poll_message(cx)).map_err(|e| panic!("{}", e));
    let connection = stream.forward(tx).map(|r| r.unwrap());
    tokio::spawn(connection);

    // FIXME: remove endless loop and keep client alive properly
    tokio::spawn(async move {
        _ = client
            .batch_execute(&format!("listen {event};notify {event};"))
            .await;
        loop {}
    });

    let notifications = rx.map(|m| match m {
        AsyncMessage::Notification(n) => Ok(Event::default().event(n.channel()).data(n.payload())),
        AsyncMessage::Notice(_) => Ok(Event::default()),
        _ => Ok(Event::default()),
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
#[cfg(test)]
mod tests {
    use std::error::Error;

    use axum::Router;
    use bb8::PooledConnection;
    use bb8_postgres::PostgresConnectionManager;
    use tokio_postgres::NoTls;

    use crate::app;

    pub(crate) async fn setup_app(
        db_name: &'static str,
    ) -> Result<
        (
            PooledConnection<'static, PostgresConnectionManager<NoTls>>,
            Router,
        ),
        Box<dyn Error>,
    > {
        let manager = PostgresConnectionManager::new_from_stringlike(
            "postgres://localhost:28817/postgres",
            NoTls,
        )?;
        let pool = bb8::Pool::builder().build(manager).await?;
        let conn = pool.get().await?;
        conn.execute(&format!(r#"drop database if exists "{db_name}";"#), &[])
            .await?;
        conn.execute(&format!(r#"create database "{db_name}";"#), &[])
            .await?;

        let manager = PostgresConnectionManager::new_from_stringlike(
            &format!("postgres://localhost:28817/{db_name}"),
            NoTls,
        )?;
        let pool = Box::leak(Box::new(bb8::Pool::builder().build(manager).await?));
        let conn = pool.get().await?;
        conn.batch_execute(concat!(
            "create extension tankard;",
            include_str!("../../db/sql/users.sql"),
            include_str!("../../db/sql/html.sql"),
        ))
        .await?;

        let app = app(pool).await?;

        Ok((conn, app))
    }

    // FIXME: test never completes
    // #[tokio::test]
    // async fn users_listen() -> Result<(), Box<dyn Error>> {
    //     let (conn, app) = setup_app("7c1571da-1028-4dc3-b18e-e84842003f10").await?;
    //     conn.batch_execute(
    //         "insert into users (username, salt, passhash) values ('one', '', ''), ('two', '', ''), ('three', '', '');",
    //     ).await?;

    //     let response = app
    //         .oneshot(
    //             Request::builder()
    //                 .uri("/listen/users_event")
    //                 .header("Accept", "*/*")
    //                 .body(Body::empty())?,
    //         )
    //         .await?;

    //     assert_eq!(response.status(), StatusCode::OK);

    //     conn.batch_execute("insert into users (username, salt, passhash) values ('four', '', '');")
    //         .await?;

    //     assert_eq!(
    //         response.into_data_stream().next().await.unwrap()?,
    //         "event: users_event\ndata: \n\n"
    //     );

    //     Ok(())
    // }
}
