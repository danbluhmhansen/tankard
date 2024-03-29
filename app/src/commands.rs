use std::error::Error;

use amqprs::{
    channel::{BasicAckArguments, BasicPublishArguments, Channel},
    consumer::AsyncConsumer,
    BasicProperties, Deliver,
};
use axum::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{Exchange, Queue};

#[derive(Deserialize, Serialize)]
pub(crate) struct InitUser {
    pub(crate) username: String,
    pub(crate) password: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct InitGame {
    pub(crate) id: Uuid,
    pub(crate) user_id: Uuid,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct SetGame {
    pub(crate) id: Uuid,
    pub(crate) name: Option<String>,
    pub(crate) description: Option<Option<String>>,
}

#[derive(Deserialize, Serialize)]
pub(crate) enum Command {
    RefreshUsers,
    RefreshGames,
    InitUser(InitUser),
    InitGames(Vec<InitGame>),
    SetGames(Vec<SetGame>),
    DropGames(Vec<Uuid>),
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
    pub(crate) pool: PgPool,
}

impl AppConsumer {
    pub(crate) fn new(pool: PgPool) -> Self {
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
            Ok((Command::RefreshUsers, _)) => {}
            Ok((Command::RefreshGames, _)) => {}
            Ok((Command::InitUser(init_user), _)) => {
                let _ = sqlx::query!(
                    "SELECT id FROM init_users($1);",
                    serde_json::to_value([init_user]).unwrap()
                )
                .fetch_all(&self.pool)
                .await;
                let _ = Command::RefreshUsers
                    .publish(channel, Queue::Sse, Exchange::Sse)
                    .await;
            }
            Ok((Command::InitGames(games), _)) => {
                let _ = sqlx::query!(
                    "SELECT id FROM init_games($1);",
                    serde_json::to_value(games).unwrap()
                )
                .fetch_all(&self.pool)
                .await;
                let _ = Command::RefreshGames
                    .publish(channel, Queue::Sse, Exchange::Sse)
                    .await;
            }
            Ok((Command::SetGames(games), _)) => {
                let _ = sqlx::query!(
                    "SELECT id FROM set_games($1);",
                    serde_json::to_value(games).unwrap()
                )
                .fetch_all(&self.pool)
                .await;
                let _ = Command::RefreshGames
                    .publish(channel, Queue::Sse, Exchange::Sse)
                    .await;
            }
            Ok((Command::DropGames(ids), _)) => {
                let _ = sqlx::query!("SELECT id FROM drop_games($1);", &ids)
                    .fetch_all(&self.pool)
                    .await;
                let _ = Command::RefreshGames
                    .publish(channel, Queue::Sse, Exchange::Sse)
                    .await;
            }
            Err(_) => {}
        }

        let _ = channel
            .basic_ack(BasicAckArguments::new(deliver.delivery_tag(), false))
            .await;
    }
}
