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
                if let Ok(content) = Command::RefreshUsers.try_into() {
                    let _ = channel
                        .basic_publish(
                            BasicProperties::default(),
                            content,
                            BasicPublishArguments::new(Exchange::Sse.into(), Queue::Sse.into()),
                        )
                        .await;
                }
            }
            Ok(Command::RefreshGames) => {
                let _ = sqlx::query!("REFRESH MATERIALIZED VIEW games;")
                    .fetch_all(&self.pool)
                    .await;
                if let Ok(content) = Command::RefreshGames.try_into() {
                    let _ = channel
                        .basic_publish(
                            BasicProperties::default(),
                            content,
                            BasicPublishArguments::new(Exchange::Sse.into(), Queue::Sse.into()),
                        )
                        .await;
                }
            }
            Ok(Command::InitUser { username, password }) => {
                let _ = sqlx::query!(
                            "SELECT id FROM init_users(ARRAY[ROW($1, $2, gen_random_uuid())]::init_users_input[]);",
                            username,
                            password
                        )
                        .fetch_all(&self.pool)
                        .await;
                if let Ok(content) = Command::RefreshUsers.try_into() {
                    let _ = channel
                        .basic_publish(
                            BasicProperties::default(),
                            content,
                            BasicPublishArguments::new("", Queue::Db.into()),
                        )
                        .await;
                }
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
                if let Ok(content) = Command::RefreshGames.try_into() {
                    let _ = channel
                        .basic_publish(
                            BasicProperties::default(),
                            content,
                            BasicPublishArguments::new("", Queue::Db.into()),
                        )
                        .await;
                }
            }
            _ => {}
        }

        let _ = channel
            .basic_ack(BasicAckArguments::new(deliver.delivery_tag(), false))
            .await;
    }
}
