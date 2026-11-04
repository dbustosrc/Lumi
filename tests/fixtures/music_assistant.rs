use super::{MIN_SCHEMA_VERSION, MusicAssistantSettings, SessionError, run_session};

use std::time::Duration;

use crate::application::events::create_event_channel;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn handshake_rejects_unsupported_schema_before_authentication() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        socket
            .send(Message::text(
                json!({
                    "schema_version": MIN_SCHEMA_VERSION - 1,
                    "base_url": format!("http://{address}")
                })
                .to_string(),
            ))
            .await
            .unwrap();

        let next = tokio::time::timeout(Duration::from_secs(2), socket.next())
            .await
            .unwrap();
        assert!(!matches!(next, Some(Ok(Message::Text(_)))));
    });

    let config = MusicAssistantSettings {
        is_enabled: true,
        server_url: format!("http://{address}"),
        token: "must-not-be-sent".into(),
        queue_id: String::new(),
    };
    let (events, _receiver) = create_event_channel();
    let (_command_sender, mut commands) = mpsc::unbounded_channel();

    let result = run_session(&config, &events, &mut commands).await;

    assert!(matches!(
        result,
        Err(SessionError::Protocol(
            "Music Assistant schema 31 or newer is required"
        ))
    ));
    server.await.unwrap();
}
