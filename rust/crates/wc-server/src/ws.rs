//! `/ws?ns=<namespace>`: one plain-WebSocket session per namespace, per contract/README.md.

use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Notify;
use tokio::sync::broadcast::error::RecvError;
use tokio::time::{Instant, interval_at, sleep_until};
use wc_machine::Event;
use wc_protocol::main::ActivateModal;
use wc_protocol::{ClientEvent, Envelope, Frame, Namespace, Push};

use crate::AppState;

#[derive(Deserialize)]
pub(crate) struct WsQuery {
    ns: Option<String>,
}

pub(crate) async fn upgrade(
    State(state): State<AppState>,
    Query(query): Query<WsQuery>,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    let Some(ns) = query.ns.and_then(|ns| ns.parse::<Namespace>().ok()) else {
        return (StatusCode::BAD_REQUEST, "unknown or missing ns").into_response();
    };
    match upgrade {
        Ok(upgrade) => upgrade.on_upgrade(move |socket| session(socket, ns, state)),
        Err(_) => (StatusCode::BAD_REQUEST, "WebSocket upgrade required").into_response(),
    }
}

fn received(state: &AppState, ns: Namespace, sid: &str, text: &str) {
    let Some(frame) = Frame::decode(text).filter(|f| f.ns == ns.path()) else {
        return;
    };
    match ClientEvent::from_frame(&frame) {
        Some(ClientEvent::Action(action)) if action.command == "TurnOffRPI" && !state.platform.can_turn_off => {
            tracing::warn!(sid, "refused TurnOffRPI: this computer is not a Raspberry Pi");
            let alert = Push::message(Envelope::json(&ActivateModal::alert(crate::NO_POWER_OFF)));
            state.machine.broadcast(alert.to_client(sid));
        }
        Some(ClientEvent::Action(action)) => state.machine.send(Event::Action(action)),
        Some(ClientEvent::SettingRequest { section, setting }) => {
            state.machine.send(Event::SettingRequest { section, setting })
        }
        Some(ClientEvent::UpdateSetting { setting, value }) => {
            state.machine.send(Event::UpdateSetting { setting, value })
        }
        Some(ClientEvent::CheckForGcodeUpdate) => state.machine.send(Event::CheckForGcodeUpdate),
        Some(ClientEvent::CheckForBoardUpdate) => state.machine.send(Event::CheckForBoardUpdate),
        None => tracing::debug!(event = frame.event, "ignoring unknown event"),
    }
}

fn is_for(push: &Push, ns: Namespace, sid: &str) -> bool {
    push.ns == ns && push.client.as_deref().is_none_or(|client| client == sid)
}

async fn session(socket: WebSocket, ns: Namespace, state: AppState) {
    let sid = uuid::Uuid::new_v4().simple().to_string();
    let mut pushes = state.machine.subscribe();
    let mut closing = state.closing.subscribe();
    let (mut tx, mut rx) = socket.split();
    if tx.send(Message::text(Frame::connect(ns, &sid).encode())).await.is_err() {
        return;
    }
    if ns == Namespace::MaslowCnc {
        state.machine.send(Event::ClientConnected { sid: sid.clone() });
    }
    // The log viewer waits for a logging state; Rust never suspends logging, so it is always active.
    let logging = Push::log(json!({"log": "state", "state": false})).frame().encode();
    if ns == Namespace::Logs && tx.send(Message::text(logging.clone())).await.is_err() {
        return;
    }
    tracing::info!(sid, ns = ns.path(), "client connected");
    let pongs = Notify::new();
    // Reading runs beside writing so a client slow to take its pushes can still Stop at once.
    let reader = async {
        while let Some(Ok(message)) = rx.next().await {
            match message {
                Message::Text(text) => received(&state, ns, &sid, text.as_str()),
                Message::Pong(_) => pongs.notify_one(),
                Message::Close(_) => break,
                _ => {}
            }
        }
    };
    let writer = async {
        let mut pings = interval_at(Instant::now() + state.ping.interval, state.ping.interval);
        let mut pong_deadline: Option<Instant> = None;
        loop {
            tokio::select! {
                push = pushes.recv() => match push {
                    Ok(push) if is_for(&push, ns, &sid) => {
                        if tx.send(Message::text(push.frame().encode())).await.is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(RecvError::Lagged(missed)) => {
                        tracing::warn!(sid, missed, "client fell behind; resending the current state");
                        if ns == Namespace::MaslowCnc {
                            state.machine.send(Event::ClientResync { sid: sid.clone() });
                        } else if tx.send(Message::text(logging.clone())).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Closed) => break,
                },
                _ = pongs.notified() => pong_deadline = None,
                _ = async { closing.wait_for(|closing| *closing).await.map(drop) } => {
                    let _ = tx.send(Message::Close(None)).await;
                    break;
                }
                _ = pings.tick() => {
                    if tx.send(Message::Ping(Default::default())).await.is_err() {
                        break;
                    }
                    pong_deadline.get_or_insert(Instant::now() + state.ping.timeout);
                }
                _ = sleep_until(pong_deadline.unwrap_or_else(Instant::now)), if pong_deadline.is_some() => {
                    tracing::info!(sid, "no pong; closing");
                    let _ = tx.send(Message::Close(None)).await;
                    break;
                }
            }
        }
    };
    tokio::select! {
        _ = reader => {}
        _ = writer => {}
    }
    tracing::info!(sid, ns = ns.path(), "client disconnected");
}
