use std::{error::Error, future::IntoFuture, net::Ipv4Addr, time::Duration};

use amqprs::channel::{
    BasicConsumeArguments, ExchangeDeclareArguments, ExchangeType, QueueDeclareArguments,
};
use axum::{extract::Request, middleware, Extension, Router};
use sqlx::{postgres::PgPoolOptions, PgPool};
use strum::IntoStaticStr;
use tokio::{join, net::TcpListener};
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
#[cfg(debug_assertions)]
use tower_livereload::LiveReloadLayer;

mod auth;
mod commands;
mod components;
mod routes;

#[derive(IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
enum Queue {
    Db,
    Sse,
}

#[derive(IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
enum Exchange {
    #[strum(serialize = "")]
    Default,
    Sse,
}

#[cfg(debug_assertions)]
fn not_htmx_predicate<T>(req: &Request<T>) -> bool {
    !req.headers().contains_key("hx-request")
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
    let amqp_url = amqp_url.as_str().try_into()?;
    let amqp = amqprs::connection::Connection::open(&amqp_url).await?;

    let channel = amqp.open_channel(None).await?;

    channel
        .queue_declare(QueueDeclareArguments::new(Queue::Db.into()))
        .await?;
    channel
        .queue_declare(QueueDeclareArguments::new(Queue::Sse.into()))
        .await?;
    channel
        .exchange_declare(ExchangeDeclareArguments::of_type(
            Exchange::Sse.into(),
            ExchangeType::Fanout,
        ))
        .await?;

    let api_pool: &'static PgPool = Box::leak(Box::new(pool.clone()));

    let app = Router::new()
        .merge(routes::games::route())
        .merge(routes::index::route())
        .merge(routes::profile::route())
        .merge(routes::signin::route())
        .merge(routes::signout::route())
        .merge(routes::signup::route())
        .merge(routes::api::route())
        .fallback_service(ServeDir::new("dist"))
        .layer(
            ServiceBuilder::new()
                .layer(middleware::from_fn(auth::auth))
                .layer(Extension(api_pool))
                .layer(Extension(channel)),
        );

    #[cfg(debug_assertions)]
    let app = app.layer(LiveReloadLayer::new().request_predicate(not_htmx_predicate));

    let http = axum::serve(
        TcpListener::bind((Ipv4Addr::new(127, 0, 0, 1), 1111)).await?,
        app,
    )
    .into_future();

    let channel = amqp.open_channel(None).await?;
    let monitor = channel.basic_consume(
        commands::AppConsumer::new(pool),
        BasicConsumeArguments::new(Queue::Db.into(), ""),
    );

    let _ = join!(http, monitor);

    Ok(())
}
