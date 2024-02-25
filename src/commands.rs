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
pub(crate) enum Command {
    RefreshUsers,
    RefreshGames,
    InitUser {
        username: String,
        password: String,
    },
    InitGame {
        id: Uuid,
        name: String,
        description: Option<String>,
    },
}

impl Command {
    pub(crate) async fn publish(
        &self,
        channel: &Channel,
        queue: Queue,
        exchange: Exchange,
    ) -> Result<(), Box<dyn Error>> {
        channel
            .basic_publish(
                BasicProperties::default(),
                self.try_into()?,
                BasicPublishArguments::new(exchange.into(), queue.into()),
            )
            .await?;
        Ok(())
    }
}

impl TryFrom<&[u8]> for Command {
    type Error = Box<bincode::ErrorKind>;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        bincode::deserialize(value)
    }
}

impl TryFrom<Command> for Vec<u8> {
    type Error = Box<bincode::ErrorKind>;

    fn try_from(value: Command) -> Result<Self, Self::Error> {
        bincode::serialize(&value)
    }
}

impl TryFrom<&Command> for Vec<u8> {
    type Error = Box<bincode::ErrorKind>;

    fn try_from(value: &Command) -> Result<Self, Self::Error> {
        bincode::serialize(value)
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
        match content.as_slice().try_into() {
            Ok(Command::RefreshUsers) => {
                let _ = sqlx::query!("REFRESH MATERIALIZED VIEW users;")
                    .fetch_all(&self.pool)
                    .await;
                let _ = Command::RefreshUsers
                    .publish(channel, Queue::Sse, Exchange::Sse)
                    .await;
            }
            Ok(Command::RefreshGames) => {
                let _ = sqlx::query!("REFRESH MATERIALIZED VIEW games;")
                    .fetch_all(&self.pool)
                    .await;
                let _ = Command::RefreshGames
                    .publish(channel, Queue::Sse, Exchange::Sse)
                    .await;
            }
            Ok(Command::InitUser { username, password }) => {
                let _ = sqlx::query!(
                            "SELECT id FROM init_users(ARRAY[ROW($1, $2, gen_random_uuid())]::init_users_input[]);",
                            username,
                            password
                        )
                        .fetch_all(&self.pool)
                        .await;
                let _ = Command::RefreshUsers
                    .publish(channel, Queue::Db, Exchange::Default)
                    .await;
            }
            Ok(Command::InitGame {
                id,
                name,
                description,
            }) => {
                let _ = sqlx::query!(
                            "SELECT id FROM init_games(ARRAY[ROW($1, $2, $3, gen_random_uuid())]::init_games_input[]);",
                            id,
                            name,
                            description
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
