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
    auth,
    commands::Command,
    components::{boost, ARTICLE, BTN, BTN_ERR, BTN_WARN, DIALOG},
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
        section
            x-data="{ games: [], setGame: {} }"
            x-init="games = await fetch('/api/games').then(res => res.json())"
            class="flex flex-col gap-4 items-center"
        {
            dialog #(Submit::Save) class=(DIALOG) {
                article class=(ARTICLE) {
                    header { h1 class="text-xl" { "Add game" } }
                    div class="flex flex-col p-2" {
                        label class="flex flex-col gap-2" {
                            span { "Name" }
                            input
                                type="text"
                                name="name"
                                required
                                autofocus
                                x-model="setGame.name"
                                class="p-1 bg-transparent rounded border";
                        }
                        label class="flex flex-col gap-2" {
                            span { "Description" }
                            input
                                type="textarea"
                                name="description"
                                x-model="setGame.description"
                                class="p-1 bg-transparent rounded border";
                        }
                    }
                    footer class="flex gap-1 justify-end" {
                        a href="#" hx-boost="false" class=(BTN) { span class="i-tabler-arrow-back size-6"; }
                        a
                            href="#"
                            hx-boost="false"
                            "@click"="if (!games.some(g => g.id === setGame.id)) games.push(setGame)"
                            class=(BTN)
                        { span class="i-tabler-plus size-6"; }
                    }
                }
            }
            h1 class="text-xl" { "Games" }
            div class="rounded bg-slate-100 min-w-80 dark:bg-slate-800" {
                div class="flex flex-col p-2" {}
                table class="w-full" {
                    colgroup { col width="1%"; col; }
                    thead {
                        tr class="border-b first:border-t border-slate-200 dark:border-slate-700" {
                            th class="p-2 text-center" {
                                div role="group" class="flex gap-1 w-fit" {
                                    button
                                        "@click"="Tankard.gamesSubmit(games)"
                                        class=(BTN)
                                    { span class="i-tabler-check size-6"; }
                                    a
                                        href={"#" (Submit::Save)}
                                        hx-boost="false"
                                        "@click"="
                                            setGame = { id: crypto.randomUUID(), name: '', description: '', new: true }
                                        "
                                        class=(BTN)
                                    { span class="i-tabler-plus size-6"; }
                                }
                            }
                            th class="p-2 text-center" { "Name" }
                        }
                    }
                    tfoot { tr { td class="p-2 text-center" {} } }
                    tbody {
                        template x-for="game in games" {
                            tr
                                x-init="game.old = { ...game }"
                                ":class"="
                                    game.drop
                                        ? 'bg-red-500'
                                        : game.new
                                            ? 'bg-green-500'
                                            : game.set && 'bg-orange-500'"
                                class="border-b border-slate-200 dark:border-slate-700"
                            {
                                td class="p-2 text-center" {
                                    div role="group" class="flex gap-1 w-fit" {
                                        button
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
                                            class=(BTN_WARN)
                                        {
                                            span
                                                ":class"="game.set ? 'i-tabler-arrow-back' : 'i-tabler-pencil'"
                                                class="size-6";
                                        }
                                        button "@click"="game.drop = !game.drop" class=(BTN_ERR)
                                        {
                                            span
                                                ":class"="game.drop ? 'i-tabler-arrow-back' : 'i-tabler-trash'"
                                                class="size-6";
                                        }
                                    }
                                }
                                td x-text="game.name" class="p-2 text-center" {}
                            }
                        }
                        tr x-show="games.length === 0" class="border-b border-slate-200 dark:border-slate-700" {
                            td colspan="9" class="p-2 text-center" { "No games..." }
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
