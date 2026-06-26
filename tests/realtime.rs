use std::{net::SocketAddr, time::Duration};

use axum::{body::Body, http::Request};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use skenion_runtime::{RuntimeServerState, runtime_router_with_state};
use tokio::{net::TcpListener, task::JoinHandle, time::timeout};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Error as TungsteniteError, Message},
};
use tower::ServiceExt;

type TestSocket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

struct TestRuntime {
    addr: SocketAddr,
    handle: JoinHandle<()>,
}

impl Drop for TestRuntime {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

async fn spawn_runtime() -> TestRuntime {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("test listener binds");
    let addr = listener.local_addr().expect("test listener has local addr");
    let app = runtime_router_with_state(RuntimeServerState::default());
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("runtime serves");
    });
    TestRuntime { addr, handle }
}

async fn connect_session(runtime: &TestRuntime, session_id: &str) -> TestSocket {
    let url = format!("ws://{}/v0/sessions/{session_id}", runtime.addr);
    let (socket, _) = connect_async(url).await.expect("websocket connects");
    socket
}

async fn send_json(socket: &mut TestSocket, value: Value) {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .expect("websocket send succeeds");
}

async fn next_json(socket: &mut TestSocket) -> Value {
    loop {
        let message = timeout(Duration::from_secs(2), socket.next())
            .await
            .expect("websocket frame arrives")
            .expect("websocket remains open")
            .expect("websocket frame succeeds");
        match message {
            Message::Text(text) => {
                return serde_json::from_str(text.as_ref()).expect("frame is JSON");
            }
            Message::Ping(_) | Message::Pong(_) => continue,
            Message::Close(close) => panic!("websocket closed unexpectedly: {close:?}"),
            Message::Binary(_) | Message::Frame(_) => panic!("unexpected websocket frame"),
        }
    }
}

async fn next_type(socket: &mut TestSocket, message_type: &str) -> Value {
    loop {
        let frame = next_json(socket).await;
        if frame["type"] == message_type {
            return frame;
        }
    }
}

async fn attach(socket: &mut TestSocket, message_id: &str, last_cursor: Option<&str>) -> Value {
    attach_with_resume(socket, message_id, last_cursor, None).await
}

async fn attach_with_resume(
    socket: &mut TestSocket,
    message_id: &str,
    last_cursor: Option<&str>,
    resume_token: Option<&str>,
) -> Value {
    let mut payload = json!({
        "clientId": "client-hint",
        "windowId": "window-hint",
        "hints": { "label": "test" }
    });
    if let Some(last_cursor) = last_cursor {
        payload["lastCursor"] = Value::String(last_cursor.to_owned());
    }
    if let Some(resume_token) = resume_token {
        payload["resumeToken"] = Value::String(resume_token.to_owned());
    }
    send_json(
        socket,
        json!({
            "schema": "skenion.runtime.realtime",
            "schemaVersion": "0.1.0",
            "type": "session.hello",
            "messageId": message_id,
            "sessionId": "default",
            "clientId": "client-hint",
            "windowId": "window-hint",
            "payload": payload
        }),
    )
    .await;
    next_json(socket).await
}

async fn send_presence(socket: &mut TestSocket, message_id: &str, idempotency_key: &str) {
    send_json(
        socket,
        json!({
            "schema": "skenion.runtime.realtime",
            "schemaVersion": "0.1.0",
            "type": "presence.update",
            "messageId": message_id,
            "sessionId": "default",
            "commandId": message_id,
            "correlationId": message_id,
            "idempotencyKey": idempotency_key,
            "payload": {
                "ttlMs": 30000,
                "presence": {
                    "state": "active",
                    "selection": { "nodeIds": ["value_1"] }
                }
            }
        }),
    )
    .await;
}

#[tokio::test]
async fn websocket_attach_returns_server_issued_identity_snapshot_and_cursor() {
    let app = runtime_router_with_state(RuntimeServerState::default());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v0/sessions/default")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("plain GET returns response");
    assert_eq!(response.status(), 426);

    let runtime = spawn_runtime().await;
    let mut socket = connect_session(&runtime, "default").await;
    let attached = attach(&mut socket, "hello-1", None).await;

    assert_eq!(attached["type"], "session.attached");
    assert_eq!(attached["schema"], "skenion.runtime.realtime");
    assert_eq!(attached["sessionId"], "default");
    assert_ne!(attached["clientId"], "client-hint");
    assert_ne!(attached["windowId"], "window-hint");
    assert!(attached["connectionId"].as_str().is_some());
    assert!(attached["payload"]["resumeToken"].as_str().is_some());
    assert!(attached["payload"]["currentRevisions"]["sessionRevision"].is_u64());
    assert!(attached["payload"]["snapshot"].is_object());
    assert!(attached["payload"]["globalCursor"].as_str().is_some());
}

#[tokio::test]
async fn two_clients_receive_presence_broadcast() {
    let runtime = spawn_runtime().await;
    let mut client_a = connect_session(&runtime, "default").await;
    let mut client_b = connect_session(&runtime, "default").await;
    let attached_a = attach(&mut client_a, "hello-a", None).await;
    let attached_b = attach(&mut client_b, "hello-b", None).await;

    assert_eq!(attached_a["type"], "session.attached");
    assert_eq!(attached_b["type"], "session.attached");

    send_presence(&mut client_a, "presence-a-1", "presence-key-a").await;
    let ack = next_type(&mut client_a, "command.ack").await;
    let broadcast = next_type(&mut client_b, "presence.updated").await;

    assert_eq!(ack["payload"]["accepted"], true);
    assert_eq!(broadcast["clientId"], attached_a["clientId"]);
    assert_eq!(broadcast["windowId"], attached_a["windowId"]);
    assert_eq!(
        broadcast["payload"]["presence"]["presence"]["state"],
        "active"
    );
    assert!(broadcast["cursor"].as_str().is_some());
}

#[tokio::test]
async fn reconnect_with_in_window_cursor_replays_missed_event() {
    let runtime = spawn_runtime().await;
    let mut producer = connect_session(&runtime, "default").await;
    let attached = attach(&mut producer, "hello-producer", None).await;
    let initial_cursor = attached["payload"]["globalCursor"]
        .as_str()
        .expect("attached includes cursor")
        .to_owned();

    send_presence(&mut producer, "presence-replay", "presence-key-replay").await;
    let _ack = next_type(&mut producer, "command.ack").await;
    let produced = next_type(&mut producer, "presence.updated").await;
    producer
        .close(None)
        .await
        .unwrap_or_else(|error: TungsteniteError| panic!("producer closes: {error}"));

    let mut reconnect = connect_session(&runtime, "default").await;
    let attached_reconnect = attach(&mut reconnect, "hello-reconnect", Some(&initial_cursor)).await;
    let replayed = next_type(&mut reconnect, "presence.updated").await;

    assert_eq!(attached_reconnect["type"], "session.attached");
    assert_eq!(replayed["cursor"], produced["cursor"]);
    assert_eq!(replayed["payload"]["replayed"], true);
}

#[tokio::test]
async fn reconnect_with_unknown_cursor_receives_sync_required() {
    let runtime = spawn_runtime().await;
    let mut socket = connect_session(&runtime, "default").await;
    let attached = attach(&mut socket, "hello-initial", None).await;
    let cursor = attached["payload"]["globalCursor"]
        .as_str()
        .expect("attached includes cursor");
    let (incarnation, _) = cursor.rsplit_once(':').expect("cursor has sequence");
    socket
        .close(None)
        .await
        .unwrap_or_else(|error: TungsteniteError| panic!("socket closes: {error}"));

    let mut reconnect = connect_session(&runtime, "default").await;
    let sync = attach(
        &mut reconnect,
        "hello-stale",
        Some(&format!("{incarnation}:999")),
    )
    .await;

    assert_eq!(sync["type"], "session.syncRequired");
    assert_eq!(
        sync["payload"]["diagnostic"]["code"],
        "realtime.cursor.unknown"
    );
    assert!(sync["payload"]["snapshot"].is_object());
}

#[tokio::test]
async fn duplicate_idempotency_key_returns_cached_ack_without_second_broadcast() {
    let runtime = spawn_runtime().await;
    let mut client_a = connect_session(&runtime, "default").await;
    let mut client_b = connect_session(&runtime, "default").await;
    let _attached_a = attach(&mut client_a, "hello-a", None).await;
    let _attached_b = attach(&mut client_b, "hello-b", None).await;

    send_presence(&mut client_a, "presence-once", "dedupe-key").await;
    let first_ack = next_type(&mut client_a, "command.ack").await;
    let _client_a_echo = next_type(&mut client_a, "presence.updated").await;
    let first_broadcast = next_type(&mut client_b, "presence.updated").await;

    send_presence(&mut client_a, "presence-duplicate", "dedupe-key").await;
    let duplicate_ack = next_type(&mut client_a, "command.ack").await;
    let no_second_broadcast = timeout(Duration::from_millis(200), next_json(&mut client_b)).await;

    assert_ne!(duplicate_ack["messageId"], first_ack["messageId"]);
    assert_eq!(duplicate_ack["connectionId"], first_ack["connectionId"]);
    assert_eq!(duplicate_ack["clientId"], first_ack["clientId"]);
    assert_eq!(duplicate_ack["windowId"], first_ack["windowId"]);
    assert_eq!(duplicate_ack["payload"]["accepted"], true);
    assert_eq!(duplicate_ack["payload"]["cached"], true);
    assert_eq!(
        duplicate_ack["payload"]["eventCursor"],
        first_ack["payload"]["eventCursor"]
    );
    assert!(first_broadcast["cursor"].as_str().is_some());
    assert!(no_second_broadcast.is_err());
}

#[tokio::test]
async fn reconnect_with_valid_resume_token_retains_identity_and_idempotency_window() {
    let runtime = spawn_runtime().await;
    let mut client_a = connect_session(&runtime, "default").await;
    let mut client_b = connect_session(&runtime, "default").await;
    let attached_a = attach(&mut client_a, "hello-a", None).await;
    let _attached_b = attach(&mut client_b, "hello-b", None).await;
    let resume_token = attached_a["payload"]["resumeToken"]
        .as_str()
        .expect("attached includes resume token")
        .to_owned();

    send_presence(
        &mut client_a,
        "presence-before-reconnect",
        "resume-dedupe-key",
    )
    .await;
    let first_ack = next_type(&mut client_a, "command.ack").await;
    let _client_a_echo = next_type(&mut client_a, "presence.updated").await;
    let first_broadcast = next_type(&mut client_b, "presence.updated").await;
    let current_cursor = first_broadcast["cursor"]
        .as_str()
        .expect("presence event includes cursor")
        .to_owned();
    client_a
        .close(None)
        .await
        .unwrap_or_else(|error: TungsteniteError| panic!("client closes: {error}"));

    let mut resumed = connect_session(&runtime, "default").await;
    let resumed_attached = attach_with_resume(
        &mut resumed,
        "hello-resume",
        Some(&current_cursor),
        Some(&resume_token),
    )
    .await;

    assert_eq!(resumed_attached["type"], "session.attached");
    assert_eq!(resumed_attached["clientId"], attached_a["clientId"]);
    assert_eq!(resumed_attached["windowId"], attached_a["windowId"]);
    assert_ne!(resumed_attached["connectionId"], attached_a["connectionId"]);
    assert_ne!(
        resumed_attached["payload"]["resumeToken"],
        attached_a["payload"]["resumeToken"]
    );

    send_presence(
        &mut resumed,
        "presence-after-reconnect",
        "resume-dedupe-key",
    )
    .await;
    let resumed_ack = next_type(&mut resumed, "command.ack").await;
    let no_second_broadcast = timeout(Duration::from_millis(200), next_json(&mut client_b)).await;

    assert_eq!(
        resumed_ack["connectionId"],
        resumed_attached["connectionId"]
    );
    assert_eq!(resumed_ack["clientId"], attached_a["clientId"]);
    assert_eq!(resumed_ack["windowId"], attached_a["windowId"]);
    assert_eq!(resumed_ack["payload"]["accepted"], true);
    assert_eq!(resumed_ack["payload"]["cached"], true);
    assert_eq!(
        resumed_ack["payload"]["eventCursor"],
        first_ack["payload"]["eventCursor"]
    );
    assert!(no_second_broadcast.is_err());
}

#[tokio::test]
async fn guessed_adjacent_resume_token_cannot_reuse_identity_or_idempotency_scope() {
    let runtime = spawn_runtime().await;
    let mut client_a = connect_session(&runtime, "default").await;
    let mut client_b = connect_session(&runtime, "default").await;
    let attached_a = attach(&mut client_a, "hello-a", None).await;
    let _attached_b = attach(&mut client_b, "hello-b", None).await;
    let cursor = attached_a["payload"]["globalCursor"]
        .as_str()
        .expect("attached includes cursor");
    let (incarnation, _) = cursor.rsplit_once(':').expect("cursor has sequence");
    let guessed_resume_token = format!("{incarnation}:resume:000001");

    send_presence(&mut client_a, "presence-before-guess", "guessed-dedupe-key").await;
    let first_ack = next_type(&mut client_a, "command.ack").await;
    let _client_a_echo = next_type(&mut client_a, "presence.updated").await;
    let _first_broadcast = next_type(&mut client_b, "presence.updated").await;

    let mut guessed = connect_session(&runtime, "default").await;
    let sync = attach_with_resume(
        &mut guessed,
        "hello-guessed-token",
        None,
        Some(&guessed_resume_token),
    )
    .await;

    assert_eq!(sync["type"], "session.syncRequired");
    assert_eq!(
        sync["payload"]["diagnostic"]["code"],
        "realtime.resume-token.invalid"
    );
    assert_ne!(sync["clientId"], attached_a["clientId"]);
    assert_ne!(sync["windowId"], attached_a["windowId"]);
    assert_ne!(sync["payload"]["resumeToken"], guessed_resume_token);

    send_presence(
        &mut guessed,
        "presence-after-guessed-token",
        "guessed-dedupe-key",
    )
    .await;
    let guessed_ack = next_type(&mut guessed, "command.ack").await;

    assert_eq!(guessed_ack["clientId"], sync["clientId"]);
    assert_eq!(guessed_ack["windowId"], sync["windowId"]);
    assert_ne!(guessed_ack["clientId"], first_ack["clientId"]);
    assert_ne!(guessed_ack["windowId"], first_ack["windowId"]);
    assert_eq!(guessed_ack["payload"]["accepted"], true);
    assert_eq!(guessed_ack["payload"]["cached"], false);
}
