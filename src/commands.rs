use amqprs::{
    channel::{BasicAckArguments, BasicPublishArguments, Channel},
    consumer::AsyncConsumer,
    BasicProperties, Deliver,
};
use axum::async_trait;
use sqlx::{Pool, Postgres};

use crate::routes::{games::InitGame, signup::InitUser};

pub(crate) struct TankardConsumer {
    pub(crate) pool: Pool<Postgres>,
}

impl TankardConsumer {
    pub(crate) fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AsyncConsumer for TankardConsumer {
    async fn consume(
        &mut self, // use `&mut self` to make trait object to be `Sync`
        channel: &Channel,
        deliver: Deliver,
        _basic_properties: BasicProperties,
        content: Vec<u8>,
    ) {
        if let Ok("rfsh-users") = std::str::from_utf8(&content) {
            let _ = sqlx::query!("REFRESH MATERIALIZED VIEW users;")
                .fetch_all(&self.pool)
                .await;
            let _ = channel
                .basic_publish(
                    BasicProperties::default(),
                    (*"rfsh-users".as_bytes()).to_vec(),
                    BasicPublishArguments::new("sse", "sse"),
                )
                .await;
        } else if let Ok("rfsh-games") = std::str::from_utf8(&content) {
            let _ = sqlx::query!("REFRESH MATERIALIZED VIEW games;")
                .fetch_all(&self.pool)
                .await;
            let _ = channel
                .basic_publish(
                    BasicProperties::default(),
                    (*"rfsh-games".as_bytes()).to_vec(),
                    BasicPublishArguments::new("sse", "sse"),
                )
                .await;
        } else if let Ok(InitUser { username, password }) = serde_json::from_slice(&content) {
            let _ = sqlx::query!(
                    "SELECT id FROM init_users(ARRAY[ROW($1, $2, gen_random_uuid())]::init_users_input[]);",
                    username,
                    password
                )
                .fetch_all(&self.pool)
                .await;
            let _ = channel
                .basic_publish(
                    BasicProperties::default(),
                    (*"rfsh-users".as_bytes()).to_vec(),
                    BasicPublishArguments::new("", "db"),
                )
                .await;
        } else if let Ok(InitGame {
            id,
            name,
            description,
        }) = serde_json::from_slice(&content)
        {
            let _ = sqlx::query!(
                    "SELECT id FROM init_games(ARRAY[ROW($1, $2, $3, gen_random_uuid())]::init_games_input[]);",
                    id,
                    name,
                    description
                )
                .fetch_all(&self.pool)
                .await;
            let _ = channel
                .basic_publish(
                    BasicProperties::default(),
                    (*"rfsh-games".as_bytes()).to_vec(),
                    BasicPublishArguments::new("", "db"),
                )
                .await;
        }

        let _ = channel
            .basic_ack(BasicAckArguments::new(deliver.delivery_tag(), false))
            .await;
    }
}
