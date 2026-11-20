use super::{
    MIN_SCHEMA_VERSION, MusicAssistantSettings, SessionCommand, SessionError, local_player_id,
    run_session,
};

use std::time::Duration;

use crate::application::events::{AppEvent, EventReceiver, create_event_channel};
use crate::domain::media::{
    CatalogCommand, CatalogResult, CatalogTrack, MediaCommand, MediaConnection, PlaybackSnapshot,
    PlaybackStatus,
};
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

#[tokio::test]
async fn blank_queue_follows_phone_without_transfer_and_controls_phone() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let local_id = local_player_id();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let local_queue = json!({
            "queue_id": local_id,
            "active": true,
            "state": "idle"
        });
        let phone_queue = |state, elapsed_time| {
            json!({
                "queue_id": "phone",
                "active": true,
                "state": state,
                "elapsed_time": elapsed_time,
                "current_item": {
                    "queue_item_id": "phone-track",
                    "name": "Phone Track",
                    "duration": 180.0
                }
            })
        };
        socket
            .send(Message::text(
                json!({
                    "schema_version": MIN_SCHEMA_VERSION,
                    "base_url": format!("http://{address}")
                })
                .to_string(),
            ))
            .await
            .unwrap();

        let auth = next_text_json(&mut socket).await;
        assert_eq!(auth["command"], "auth");
        socket
            .send(Message::text(
                json!({ "message_id": "lumi-auth", "result": true }).to_string(),
            ))
            .await
            .unwrap();

        let queues_request = next_text_json(&mut socket).await;
        assert_eq!(queues_request["command"], "player_queues/all");
        socket
            .send(Message::text(
                json!({
                    "message_id": "lumi-queues",
                    "result": [local_queue.clone(), phone_queue("playing", 8.0)]
                })
                .to_string(),
            ))
            .await
            .unwrap();
        socket
            .send(Message::text(
                json!({
                    "event": "queue_updated",
                    "data": local_queue.clone()
                })
                .to_string(),
            ))
            .await
            .unwrap();
        socket
            .send(Message::text(
                json!({
                    "event": "queue_updated",
                    "data": phone_queue("playing", 9.0)
                })
                .to_string(),
            ))
            .await
            .unwrap();
        socket
            .send(Message::text(
                json!({
                    "event": "queue_updated",
                    "data": phone_queue("paused", 10.0)
                })
                .to_string(),
            ))
            .await
            .unwrap();

        let command = next_text_json(&mut socket).await;
        assert_eq!(command["command"], "player_queues/play_pause");
        assert_eq!(command["args"]["queue_id"], "phone");
        socket.send(Message::Close(None)).await.unwrap();
    });

    let config = MusicAssistantSettings {
        is_enabled: true,
        server_url: format!("http://{address}"),
        token: "local-test-token".into(),
        queue_id: String::new(),
    };
    let (events, mut event_receiver) = create_event_channel();
    let (command_sender, mut commands) = mpsc::unbounded_channel();
    let session = tokio::spawn(async move { run_session(&config, &events, &mut commands).await });

    wait_for_playback(&mut event_receiver, PlaybackStatus::Playing).await;
    wait_for_playback(&mut event_receiver, PlaybackStatus::Paused).await;
    command_sender
        .send(SessionCommand::Transport(MediaCommand::PlayPause))
        .unwrap();

    assert!(matches!(session.await.unwrap(), Ok(())));
    server.await.unwrap();
}

#[tokio::test]
async fn catalog_search_and_play_target_local_or_configured_queue_not_followed_phone() {
    let local_id = local_player_id();
    run_catalog_session("", &local_id).await;
    run_catalog_session("configured-player", "configured-player").await;
}

async fn run_catalog_session(configured_queue: &str, expected_play_queue: &str) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let local_id = local_player_id();
    let server_configured_queue = configured_queue.to_owned();
    let config_queue_id = configured_queue.to_owned();
    let expected_play_queue = expected_play_queue.to_owned();
    let search_id = uuid::Uuid::new_v4();
    let play_id = uuid::Uuid::new_v4();
    let server_search_id = search_id.clone();
    let server_play_id = play_id.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        socket
            .send(Message::text(
                json!({
                    "schema_version": MIN_SCHEMA_VERSION,
                    "base_url": format!("http://{address}")
                })
                .to_string(),
            ))
            .await
            .unwrap();

        let auth = next_text_json(&mut socket).await;
        assert_eq!(auth["command"], "auth");
        socket
            .send(Message::text(
                json!({ "message_id": "lumi-auth", "result": true }).to_string(),
            ))
            .await
            .unwrap();

        let queues_request = next_text_json(&mut socket).await;
        assert_eq!(queues_request["command"], "player_queues/all");
        let mut queues = vec![
            json!({ "queue_id": local_id, "active": true, "state": "idle" }),
            json!({ "queue_id": "phone", "active": true, "state": "playing" }),
        ];
        if !server_configured_queue.is_empty() {
            queues.push(json!({
                "queue_id": server_configured_queue,
                "active": true,
                "state": "paused"
            }));
        }
        socket
            .send(Message::text(
                json!({ "message_id": "lumi-queues", "result": queues }).to_string(),
            ))
            .await
            .unwrap();

        let search = next_text_json(&mut socket).await;
        assert_eq!(
            search["message_id"],
            format!("lumi-search-{server_search_id}")
        );
        assert_eq!(search["command"], "music/search");
        assert_eq!(search["args"]["search_query"], "Björk");
        assert_eq!(search["args"]["media_types"], json!(["track"]));
        socket
            .send(Message::text(
                json!({
                    "message_id": format!("lumi-search-{server_search_id}"),
                    "result": {
                        "tracks": [{
                            "uri": "library://joga",
                            "name": "Jóga",
                            "artists": [{"name": "Björk"}]
                        }]
                    }
                })
                .to_string(),
            ))
            .await
            .unwrap();

        let play = next_text_json(&mut socket).await;
        assert_eq!(play["message_id"], format!("lumi-play-{server_play_id}"));
        assert_eq!(play["command"], "player_queues/play_media");
        assert_eq!(play["args"]["queue_id"], expected_play_queue);
        assert_eq!(play["args"]["media"], "library://joga");
        assert_eq!(play["args"]["option"], "play");
        socket
            .send(Message::text(
                json!({
                    "message_id": format!("lumi-play-{server_play_id}"),
                    "result": true
                })
                .to_string(),
            ))
            .await
            .unwrap();
        socket.send(Message::Close(None)).await.unwrap();
    });

    let config = MusicAssistantSettings {
        is_enabled: true,
        server_url: format!("http://{address}"),
        token: "local-test-token".into(),
        queue_id: config_queue_id,
    };
    let (events, mut event_receiver) = create_event_channel();
    let (command_sender, mut commands) = mpsc::unbounded_channel();
    let session = tokio::spawn(async move { run_session(&config, &events, &mut commands).await });

    wait_for_connected(&mut event_receiver).await;
    command_sender
        .send(SessionCommand::Catalog(CatalogCommand::Search {
            id: search_id,
            query: "Björk".into(),
        }))
        .unwrap();
    match wait_for_catalog(&mut event_receiver, search_id).await {
        CatalogResult::Tracks(tracks) => assert_eq!(
            tracks,
            vec![CatalogTrack {
                uri: "library://joga".into(),
                title: "Jóga".into(),
                artist: "Björk".into(),
            }]
        ),
        other => panic!("unexpected search result: {other:?}"),
    }

    command_sender
        .send(SessionCommand::Catalog(CatalogCommand::Play {
            id: play_id,
            uri: "library://joga".into(),
        }))
        .unwrap();
    assert!(matches!(
        wait_for_catalog(&mut event_receiver, play_id).await,
        CatalogResult::Playing
    ));

    assert!(matches!(session.await.unwrap(), Ok(())));
    server.await.unwrap();
}

async fn wait_for_connected(events: &mut EventReceiver) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if matches!(
                events.recv().await,
                Some(AppEvent::MediaConnection(MediaConnection::Connected))
            ) {
                return;
            }
        }
    })
    .await
    .unwrap();
}

async fn wait_for_catalog(events: &mut EventReceiver, id: uuid::Uuid) -> CatalogResult {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(AppEvent::MusicCatalog {
                id: result_id,
                result,
            }) = events.recv().await
                && result_id == id
            {
                return result;
            }
        }
    })
    .await
    .unwrap()
}

async fn next_text_json<S>(socket: &mut tokio_tungstenite::WebSocketStream<S>) -> serde_json::Value
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    match tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
    {
        Message::Text(text) => serde_json::from_str(text.as_str()).unwrap(),
        message => panic!("expected text frame, got {message:?}"),
    }
}

async fn wait_for_playback(
    events: &mut crate::application::events::EventReceiver,
    status: PlaybackStatus,
) -> PlaybackSnapshot {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(crate::application::events::AppEvent::Playback(snapshot)) =
                events.recv().await
                && snapshot.status == status
                && snapshot
                    .track
                    .as_ref()
                    .is_some_and(|track| track.title == "Phone Track")
            {
                return snapshot;
            }
        }
    })
    .await
    .unwrap()
}
