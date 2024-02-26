use std::error::Error;

use amqprs::{
    channel::{BasicAckArguments, BasicPublishArguments, Channel},
    consumer::AsyncConsumer,
    BasicProperties, Deliver,
};
use axum::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use uuid::Uuid;

use crate::{Exchange, Queue};

#[derive(Deserialize, Serialize)]
pub(crate) struct InitUser {
    pub(crate) username: String,
    pub(crate) password: String,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct InitGame {
    pub(crate) user_id: Uuid,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub(crate) enum Command {
    RefreshUsers,
    RefreshGames,
    InitUser(InitUser),
    InitGame(InitGame),
}

impl Command {
    pub(crate) async fn publish(
        &self,
        channel: &Channel,
        queue: Queue,
        exchange: Exchange,
    ) -> Result<(), Box<dyn Error>> {
        if let Ok(content) = bincode::serde::encode_to_vec(self, bincode::config::standard()) {
            channel
                .basic_publish(
                    BasicProperties::default(),
                    content,
                    BasicPublishArguments::new(exchange.into(), queue.into()),
                )
                .await?;
        }
        Ok(())
    }
}

pub(crate) struct AppConsumer {
    pub(crate) pool: Pool<Postgres>,
}

impl AppConsumer {
    pub(crate) fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AsyncConsumer for AppConsumer {
    async fn consume(
        &mut self, // use `&mut self` to make trait object to be `Sync`
        channel: &Channel,
        deliver: Deliver,
        _basic_properties: BasicProperties,
        content: Vec<u8>,
    ) {
        match bincode::serde::decode_from_slice(&content, bincode::config::standard()) {
            Ok((Command::RefreshUsers, _)) => {
                let _ = sqlx::query!("REFRESH MATERIALIZED VIEW users;")
                    .fetch_all(&self.pool)
                    .await;
                let _ = Command::RefreshUsers
                    .publish(channel, Queue::Sse, Exchange::Sse)
                    .await;
            }
            Ok((Command::RefreshGames, _)) => {
                let _ = sqlx::query!("REFRESH MATERIALIZED VIEW games;")
                    .fetch_all(&self.pool)
                    .await;
                let _ = Command::RefreshGames
                    .publish(channel, Queue::Sse, Exchange::Sse)
                    .await;
            }
            Ok((Command::InitUser(init_user), _)) => {
                let _ = sqlx::query!(
                    "SELECT id FROM init_users($1);",
                    serde_json::to_value([init_user]).unwrap()
                )
                .fetch_all(&self.pool)
                .await;
                let _ = Command::RefreshUsers
                    .publish(channel, Queue::Db, Exchange::Default)
                    .await;
            }
            Ok((Command::InitGame(init_game), _)) => {
                let _ = sqlx::query!(
                    "SELECT id FROM init_games($1);",
                    serde_json::to_value([init_game]).unwrap()
                )
                .fetch_all(&self.pool)
                .await;
                let _ = Command::RefreshGames
                    .publish(channel, Queue::Db, Exchange::Default)
                    .await;
            }
            Err(_) => {}
        }

        let _ = channel
            .basic_ack(BasicAckArguments::new(deliver.delivery_tag(), false))
            .await;
    }
}
