use amqprs::channel::{BasicConsumeArguments, Channel};
use axum::{
    http::StatusCode,
    middleware,
    response::{
        sse::{Event, KeepAlive},
        IntoResponse, Response, Sse,
    },
    Extension, Router,
};
use axum_extra::routing::{RouterExt, TypedPath};
use axum_htmx::HxBoosted;
use bincode::error::DecodeError;
use maud::{html, Markup};
use serde::Deserialize;
use strum::Display;
use tokio_stream::{wrappers::UnboundedReceiverStream, StreamExt};

use crate::{
    auth::{self},
    commands::Command,
    components::boost,
    Queue,
};

pub(crate) fn route() -> Router {
    Router::new()
        .typed_get(get)
        .typed_get(sse)
        .layer(middleware::from_fn(auth::req_auth))
}

const SSE_EVENT: &str = "rfsh-games";

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
        section x-data="{ games: [], setGame: {} }" x-init="games = await fetch('/api/games').then(res => res.json())" {
            dialog #(Submit::Save) {
                article {
                    header { h1 { "Add game" } }
                    div {
                        label {
                            span { "Name" }
                            input type="text" name="name" required autofocus x-model="setGame.name";
                        }
                        label {
                            span { "Description" }
                            input type="textarea" name="description" x-model="setGame.description";
                        }
                    }
                    footer {
                        a href="#" hx-boost="false" { span.tabler-arrow-back; }
                        a
                            href="#"
                            hx-boost="false"
                            "@click"="if (!games.some(g => g.id === setGame.id)) games.push(setGame)"
                        { span.tabler-plus; }
                    }
                }
            }
            h1 { "Games" }
            div {
                div {}
                table {
                    colgroup { col width="1%"; col; }
                    thead {
                        tr {
                            th {
                                div role="group" style="width: fit-content;gap: .25rem;padding: 0;" {
                                    button
                                        "@click"="
                                            if (games.some(g => g.new && !g.drop))
                                                fetch('/api/games', {
                                                    method: 'POST',
                                                    headers: { 'Content-Type': 'application/json', },
                                                    body: JSON.stringify(games.filter(g => g.new && !g.drop))
                                                });
                                            if (games.some(g => g.set))
                                                fetch('/api/games', {
                                                    method: 'PUT',
                                                    headers: { 'Content-Type': 'application/json', },
                                                    body: JSON.stringify(games.filter(g => g.set))
                                                });
                                            if (games.some(g => g.drop && !g.new))
                                                fetch(`/api/games?ids=${games.filter(g => g.drop && !g.new).map(g => g.id).join('&ids=')}`, {
                                                    method: 'DELETE',
                                                });
                                        "
                                    { span.tabler-check; }
                                    a
                                        href={"#" (Submit::Save)}
                                        hx-boost="false"
                                        "@click"="
                                            setGame = { id: crypto.randomUUID(), name: '', description: '', new: true }
                                        "
                                    { span.tabler-plus; }
                                }
                            }
                            th { "Name" }
                        }
                    }
                    tfoot { tr { td {} } }
                    tbody {
                        template x-for="game in games" {
                            tr
                                x-init="game.old = { ...game }"
                                ":style"="
                                    game.drop
                                        ? { background: 'red' }
                                        : game.new
                                            ? { background: 'green' }
                                            : game.set && { background: 'orange' }
                                " {
                                td {
                                    div role="group" style="width: fit-content;gap: .25rem;padding: 0;" {
                                        button.tertiary
                                            "@click"="
                                                if (game.set) {
                                                    Object.assign(game, game.old);   
                                                    game.old = { ...game };
                                                    game.set = false;
                                                } else {
                                                    if (!game.new) game.set = true;
                                                    setGame = game;
                                                    window.location.hash = 'save';
                                                }
                                            "
                                        { span ":class"="game.set ? 'tabler-arrow-back' : 'tabler-pencil'"; }
                                        button.secondary "@click"="game.drop = !game.drop"
                                        { span ":class"="game.drop ? 'tabler-arrow-back' : 'tabler-trash'"; }
                                    }
                                }
                                td x-text="game.name" {}
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(TypedPath)]
#[typed_path("/games")]
pub(crate) struct Path;

pub(crate) async fn get(_: Path, HxBoosted(boosted): HxBoosted) -> Markup {
    boost(page().await, true, boosted)
}

#[derive(TypedPath)]
#[typed_path("/games-sse")]
pub(crate) struct SsePath;

// TODO: re-implement sse to support the alpinejs implementation
pub(crate) async fn sse(
    _: SsePath,
    // Extension(CurrentUser { id }): Extension<CurrentUser>,
    // Extension(pool): Extension<&'static PgPool>,
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
        let stream = UnboundedReceiverStream::new(rx).map(move |msg| {
            match msg.content.as_ref().map(|content| {
                bincode::serde::decode_from_slice(content, bincode::config::standard())
            }) {
                Some(Ok((Command::RefreshGames, _))) => {
                    Ok(Event::default().event(SSE_EVENT).data(""))
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
