use std::{error::Error, future::IntoFuture, net::Ipv4Addr, time::Duration};

use axum::{extract::Request, middleware, Extension, Router};
use futures_util::StreamExt;
use lapin::{
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicPublishOptions, ExchangeDeclareOptions,
        QueueDeclareOptions,
    },
    types::FieldTable,
    BasicProperties, Channel, ExchangeKind,
};
use routes::{games::InitGame, signup::InitUser};
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use tokio::{join, net::TcpListener};
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
#[cfg(debug_assertions)]
use tower_livereload::LiveReloadLayer;

mod auth;
mod components;
mod routes;

#[cfg(debug_assertions)]
fn not_htmx_predicate<T>(req: &Request<T>) -> bool {
    !req.headers().contains_key("hx-request")
}

async fn monitor(channel: Channel, pool: &Pool<Postgres>) -> Result<(), Box<dyn Error>> {
    let mut consumer = channel
        .basic_consume(
            "db",
            "db_consumer",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;

    while let Some(delivery) = consumer.next().await {
        if let Ok(delivery) = delivery {
            if let Ok("rfsh-users") = std::str::from_utf8(&delivery.data) {
                let _ = sqlx::query!("REFRESH MATERIALIZED VIEW users;")
                    .fetch_all(pool)
                    .await;
                let _ = channel
                    .basic_publish(
                        "sse",
                        "sse",
                        BasicPublishOptions::default(),
                        "rfsh-users".as_bytes(),
                        BasicProperties::default(),
                    )
                    .await;
            } else if let Ok("rfsh-games") = std::str::from_utf8(&delivery.data) {
                let _ = sqlx::query!("REFRESH MATERIALIZED VIEW games;")
                    .fetch_all(pool)
                    .await;
                let _ = channel
                    .basic_publish(
                        "sse",
                        "sse",
                        BasicPublishOptions::default(),
                        "rfsh-games".as_bytes(),
                        BasicProperties::default(),
                    )
                    .await;
            } else if let Ok(InitUser { username, password }) =
                serde_json::from_slice(&delivery.data)
            {
                let _ = sqlx::query!(
                        "SELECT id FROM init_users(ARRAY[ROW($1, $2, gen_random_uuid())]::init_users_input[]);",
                        username,
                        password
                    )
                    .fetch_all(pool)
                    .await;
                let _ = channel
                    .basic_publish(
                        "",
                        "db",
                        BasicPublishOptions::default(),
                        "rfsh-users".as_bytes(),
                        BasicProperties::default(),
                    )
                    .await;
            } else if let Ok(InitGame {
                id,
                name,
                description,
            }) = serde_json::from_slice(&delivery.data)
            {
                let _ = sqlx::query!(
                        "SELECT id FROM init_games(ARRAY[ROW($1, $2, $3, gen_random_uuid())]::init_games_input[]);",
                        id,
                        name,
                        description
                    )
                    .fetch_all(pool)
                    .await;
                let _ = channel
                    .basic_publish(
                        "",
                        "db",
                        BasicPublishOptions::default(),
                        "rfsh-games".as_bytes(),
                        BasicProperties::default(),
                    )
                    .await;
            }
            let _ = delivery.ack(BasicAckOptions::default()).await;
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let db_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&db_url)
        .await?;

    let amqp_url = std::env::var("AMQP_URL")?;
    let amqp =
        lapin::Connection::connect(&amqp_url, lapin::ConnectionProperties::default()).await?;

    let channel = amqp.create_channel().await?;

    channel
        .queue_declare("db", QueueDeclareOptions::default(), FieldTable::default())
        .await?;
    channel
        .queue_declare("sse", QueueDeclareOptions::default(), FieldTable::default())
        .await?;
    channel
        .exchange_declare(
            "sse",
            ExchangeKind::Fanout,
            ExchangeDeclareOptions::default(),
            FieldTable::default(),
        )
        .await?;

    let app = Router::new()
        .merge(routes::games::route())
        .merge(routes::index::route())
        .merge(routes::profile::route())
        .merge(routes::signin::route())
        .merge(routes::signout::route())
        .merge(routes::signup::route())
        .fallback_service(ServeDir::new("dist"))
        .layer(
            ServiceBuilder::new()
                .layer(middleware::from_fn(auth::auth))
                .layer(Extension(pool.clone()))
                .layer(Extension(channel)),
        );

    #[cfg(debug_assertions)]
    let app = app.layer(LiveReloadLayer::new().request_predicate(not_htmx_predicate));

    let http = axum::serve(
        TcpListener::bind((Ipv4Addr::new(127, 0, 0, 1), 1111)).await?,
        app,
    )
    .into_future();

    let _ = join!(http, monitor(amqp.create_channel().await?, &pool));

    Ok(())
}
