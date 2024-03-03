use amqprs::channel::{BasicConsumeArguments, Channel};
use axum::{
    http::StatusCode,
    middleware,
    response::{
        sse::{Event, KeepAlive},
        IntoResponse, Redirect, Response, Sse,
    },
    Extension, Router,
};
use axum_extra::{
    extract::Form,
    routing::{RouterExt, TypedPath},
};
use axum_htmx::{HxBoosted, HxRequest};
use bincode::error::DecodeError;
use maud::{html, Markup};
use serde::Deserialize;
use sqlx::PgPool;
use strum::Display;
use tokio_stream::{wrappers::UnboundedReceiverStream, StreamExt};
use uuid::Uuid;

use crate::{
    auth::{self, CurrentUser},
    commands::{Command, InitGame},
    components::boost,
    Exchange, Queue,
};

pub(crate) fn route() -> Router {
    Router::new()
        .typed_get(partial)
        .typed_get(get)
        .typed_post(post)
        .typed_get(sse)
        .layer(middleware::from_fn(auth::req_auth))
}

const SSE_EVENT: &str = "rfsh-games";

pub(crate) async fn table(user_id: Uuid, pool: &PgPool) -> Markup {
    let games = sqlx::query!(
        "SELECT id, name, description FROM games WHERE user_id = $1;",
        user_id
    )
    .fetch_all(pool)
    .await;
    html! {
        @if let Ok(games) = games {
            @for (id, name, description) in games.into_iter().map(|g| (g.id, g.name, g.description.unwrap_or("".to_string()))) {
                tr
                    x-init=(format!("games['{id}'] = {{ name: '{name}', description: '{description}' }}"))
                    "@click"=(format!("
                        game = {{ id: '{id}', name: '{name}', description: '{description}' }};
                        window.location.hash = '{}';
                    ", Submit::Save))
                    ":style"=(format!("
                        games['{id}']?.drop
                            ? {{ background: 'red' }}
                            : games['{id}']?.old && {{ background: 'orange' }}")) {
                    td {
                        div role="group" style="width: fit-content;gap: .25rem;padding: 0;" {
                            button.tertiary
                                type="button"
                                "@click.stop"=(format!("
                                    if (games['{id}'].old) games['{id}'] = games['{id}'].old
                                    else {{
                                        game = {{ id: '{id}', name: '{name}', description: '{description}' }};
                                        window.location.hash = '{}';
                                    }}", Submit::Save))
                            { span ":class"=(format!("games['{id}'].old ? 'tabler-arrow-back' : 'tabler-pencil'")); }
                            button.secondary
                                type="button"
                                "@click.stop"=(format!("games['{id}'].drop = !games['{id}'].drop")) {
                                span.tabler-trash;
                            }
                        }
                    }
                    td { span x-text=(format!("games['{id}'].name")); }
                }
            }
        } @else {
            tr { td colspan="100%" { "No games" } }
        }
    }
}

#[derive(Deserialize, Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub(crate) enum Submit {
    Save,
    Set,
    Drop,
}

pub(crate) async fn page() -> Markup {
    html! {
        section x-data="{ games: {}, game: { id: '', name: '', description: '' } }" {
            dialog #(Submit::Save) {
                article {
                    header { h1 { "Add game" } }
                    form #{(Submit::Save) "-form"} method="post" {
                        label {
                            span { "Name" }
                            input type="text" name="name" required autofocus x-model="game.name";
                        }
                        label {
                            span { "Description" }
                            input type="textarea" name="description" x-model="game.description";
                        }
                    }
                    footer {
                        a href="#" hx-boost="false" { "Cancel" }
                        button
                            type="button"
                            "@click"="
                                games[game.id] = { ...game, old: game.old ?? game };
                                window.location.hash = '';"
                        { "Add" }
                    }
                }
            }
            h1 { "Games" }
            form method="post" {
                div {}
                table {
                    colgroup { col width="1%"; col; }
                    thead {
                        tr {
                            th {
                                a
                                    href={"#" (Submit::Save)}
                                    hx-boost="false"
                                    "@click"="game = { id: crypto.randomUUID(), name: '', description: '' }" {
                                    span.tabler-plus;
                                }
                            }
                            th { "Name" }
                        }
                    }
                    tfoot { tr { td {} } }
                    tbody
                        hx-get=(PartialPath)
                        hx-trigger="revealed"
                        hx-ext="sse"
                        sse-connect=(SsePath)
                        sse-swap=(SSE_EVENT) {
                        tr.htmx-indicator {
                            td colspan="100%" { span.svg-spinners-gooey-balls-2 style="width: 32px;height: 32px"; }
                        }
                    }
                }
            }
        }
    }
}

#[derive(TypedPath)]
#[typed_path("/games-prtl")]
pub(crate) struct PartialPath;

pub(crate) async fn partial(
    _: PartialPath,
    HxRequest(is_hx): HxRequest,
    Extension(CurrentUser { id }): Extension<CurrentUser>,
    Extension(pool): Extension<&'static PgPool>,
) -> Response {
    if is_hx {
        table(id, &pool).await.into_response()
    } else {
        Redirect::to(Path.to_uri().path()).into_response()
    }
}

#[derive(TypedPath)]
#[typed_path("/games")]
pub(crate) struct Path;

pub(crate) async fn get(_: Path, HxBoosted(boosted): HxBoosted) -> Markup {
    boost(page().await, true, boosted)
}

#[derive(Deserialize)]
pub(crate) struct Payload {
    submit: Submit,
    name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    ids: Vec<Uuid>,
}

pub(crate) async fn post(
    _: Path,
    HxBoosted(boosted): HxBoosted,
    Extension(CurrentUser { id }): Extension<CurrentUser>,
    Extension(channel): Extension<Channel>,
    Form(Payload {
        submit,
        name,
        description,
        ids,
    }): Form<Payload>,
) -> Markup {
    match submit {
        Submit::Save => {
            let _ = Command::InitGame(InitGame {
                user_id: id,
                name: name.unwrap(),
                description,
            })
            .publish(&channel, Queue::Db, Exchange::Default)
            .await;
        }
        Submit::Set => {}
        Submit::Drop => {
            let _ = Command::DropGames(ids)
                .publish(&channel, Queue::Db, Exchange::Default)
                .await;
        }
    }
    boost(page().await, true, boosted)
}

#[derive(TypedPath)]
#[typed_path("/games-sse")]
pub(crate) struct SsePath;

pub(crate) async fn sse(
    _: SsePath,
    Extension(CurrentUser { id }): Extension<CurrentUser>,
    Extension(pool): Extension<&'static PgPool>,
    Extension(channel): Extension<Channel>,
) -> Response {
    if let Ok((_, rx)) = channel
        .basic_consume_rx(
            BasicConsumeArguments::new(Queue::Sse.into(), "")
                .auto_ack(true)
                .finish(),
        )
        .await
    {
        let table = table(id, pool).await.into_string();

        let stream = UnboundedReceiverStream::new(rx).map(move |msg| {
            match msg.content.as_ref().map(|content| {
                bincode::serde::decode_from_slice(content, bincode::config::standard())
            }) {
                Some(Ok((Command::RefreshGames, _))) => {
                    Ok(Event::default().event(SSE_EVENT).data(table.clone()))
                }
                Some(Err(err)) => Err(err),
                _ => Err(DecodeError::Other("empty message")),
            }
        });

        Sse::new(stream)
            .keep_alive(KeepAlive::default())
            .into_response()
    } else {
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    }
}
