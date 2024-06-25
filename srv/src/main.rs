use std::{error::Error, time::Duration};

use axum::{extract::State, http::StatusCode, response::Html, routing::get, Router};
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::net::TcpListener;

fn internal_error<E>(err: E) -> (StatusCode, String)
where
    E: std::error::Error,
{
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}

async fn index(State(pool): State<PgPool>) -> Result<Html<String>, (StatusCode, String)> {
    sqlx::query_scalar("select html_minify(html(html_users()));")
        .fetch_one(&pool)
        .await
        .map(Html)
        .map_err(internal_error)
}

fn app(pool: PgPool) -> Router {
    Router::new().route("/", get(index)).with_state(pool)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let db_url = std::option_env!("DATABASE_URL").unwrap_or("pg://localhost:28816/tankard");

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
    async fn index(pool: PgPool) -> Result<(), Box<dyn Error>> {
        pool.execute("create extension tankard;").await?;
        pool.execute(include_str!("../../db/sql/users.sql")).await?;
        pool.execute(include_str!("../../db/sql/html.sql")).await?;
        pool.execute("insert into users (username, salt, passhash) values ('one', '', ''), ('two', '', ''), ('three', '', '');").await?;

        let app = app(pool);

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty())?)
            .await?;

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await?.to_bytes();
        let body = String::from_utf8(body.into())?;

        let html = scraper::Html::parse_document(&body);

        assert_eq!(
            Some("<table><thead><tr><th>Username</th></tr></thead><tbody><tr><td>one</td></tr><tr><td>two</td></tr><tr><td>three</td></tr></tbody></table>"),
            html.select(&scraper::Selector::parse("table")?)
                .next()
                .map(|el| el.html())
                .as_deref()
        );

        Ok(())
    }
}
