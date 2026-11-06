use super::{TASKS_PATH, serve};

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    application::{
        events::{AppEvent, EventReceiver, create_event_channel},
        todo_service::TodoService,
    },
    domain::ports::TaskRepository,
    infrastructure::{task_store::JsonTaskRepository, task_tokens::TaskTokens},
};
use reqwest::{Client, Method, RequestBuilder, StatusCode};
use serde_json::{Value, json};
use tokio::{net::TcpListener, task::JoinHandle};
use uuid::Uuid;

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("lumi-task-api-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Api {
    client: Client,
    base_url: String,
    token: String,
    token_id: Uuid,
    tokens: Arc<TaskTokens>,
    token_path: PathBuf,
    task_path: PathBuf,
    _directory: TempDirectory,
    server: JoinHandle<()>,
    worker: JoinHandle<()>,
}

impl Api {
    async fn start() -> Self {
        let directory = TempDirectory::new();
        let token_path = directory.path().join("tokens.json");
        let task_path = directory.path().join("tasks.json");
        let tokens = Arc::new(TaskTokens::load(token_path.clone()).unwrap());
        let issued = tokens.generate().unwrap();
        let (events, receiver) = create_event_channel();

        let worker_task_path = task_path.clone();
        let worker = tokio::spawn(async move {
            task_worker(receiver, worker_task_path).await;
        });

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(serve(listener, tokens.clone(), events));

        Self {
            client: Client::new(),
            base_url: format!("http://{address}"),
            token: issued.secret,
            token_id: issued.id,
            tokens,
            token_path,
            task_path,
            _directory: directory,
            server,
            worker,
        }
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.client
            .request(method, format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
    }
}

impl Drop for Api {
    fn drop(&mut self) {
        self.server.abort();
        self.worker.abort();
    }
}

async fn task_worker(mut events: EventReceiver, task_path: PathBuf) {
    let repository = Arc::new(JsonTaskRepository::new(task_path));
    let mut service = TodoService::load(repository).unwrap();

    while let Some(event) = events.recv().await {
        let AppEvent::TaskApi { command, reply } = event else {
            continue;
        };
        let _ = reply.send(service.execute(command));
    }
}

#[tokio::test]
async fn task_api_authentication_and_token_storage_use_only_persisted_digests() {
    let api = Api::start().await;

    let unauthorized = api
        .client
        .get(format!("{}{TASKS_PATH}", api.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(unauthorized.headers()["www-authenticate"], "Bearer");

    let tokens_json = fs::read_to_string(&api.token_path).unwrap();
    assert!(!tokens_json.contains(&api.token));
    let stored: Value = serde_json::from_str(&tokens_json).unwrap();
    assert_eq!(stored[0]["info"]["id"], api.token_id.to_string());
    assert_eq!(stored[0]["digest"].as_array().unwrap().len(), 32);

    let reloaded = TaskTokens::load(api.token_path.clone()).unwrap();
    assert!(reloaded.authorizes(&api.token));
    assert_eq!(reloaded.list()[0].id, api.token_id);

    let wrong_host = api
        .request(Method::GET, TASKS_PATH)
        .header(reqwest::header::HOST, "example.invalid")
        .send()
        .await
        .unwrap();
    assert_eq!(wrong_host.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        wrong_host.json::<Value>().await.unwrap()["error"],
        "invalid_host"
    );

    api.tokens.revoke(api.token_id).unwrap();
    assert!(
        !TaskTokens::load(api.token_path.clone())
            .unwrap()
            .authorizes(&api.token)
    );

    let revoked = api.request(Method::GET, TASKS_PATH).send().await.unwrap();
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn task_api_crud_round_trips_through_the_persistent_task_repository() {
    let api = Api::start().await;

    let empty = api.request(Method::GET, TASKS_PATH).send().await.unwrap();
    assert_eq!(empty.status(), StatusCode::OK);
    assert_eq!(empty.json::<Value>().await.unwrap(), json!([]));

    let created = api
        .request(Method::POST, TASKS_PATH)
        .json(&json!({
            "title": "  Water  plants ",
            "schedule": { "date": "2026-11-03", "time": "09:30:00", "daily": true },
            "color": "blue"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let location = created.headers()[reqwest::header::LOCATION]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(location.starts_with("/v1/tasks/"));
    let created: Value = created.json().await.unwrap();
    let id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    assert_eq!(created["title"], "Water plants");
    assert_eq!(created["schedule"]["date"], "2026-11-03");
    assert_eq!(created["schedule"]["daily"], true);
    assert_eq!(created["color"], "blue");
    assert_eq!(location, format!("{TASKS_PATH}/{id}"));

    let persisted = JsonTaskRepository::new(api.task_path.clone())
        .load()
        .unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].id, id);
    assert_eq!(
        TodoService::load(Arc::new(JsonTaskRepository::new(api.task_path.clone())))
            .unwrap()
            .tasks()
            .len(),
        1
    );

    let patched = api
        .request(Method::PATCH, &format!("{TASKS_PATH}/{id}"))
        .json(&json!({ "title": "Water herbs", "is_done": true, "color": "red" }))
        .send()
        .await
        .unwrap();
    assert_eq!(patched.status(), StatusCode::OK);
    let patched: Value = patched.json().await.unwrap();
    assert_eq!(patched["title"], "Water herbs");
    assert_eq!(patched["is_done"], true);
    assert_eq!(patched["color"], "red");
    assert!(patched["completed_at"].is_string());

    let fetched = api
        .request(Method::GET, &format!("{TASKS_PATH}/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(fetched.status(), StatusCode::OK);
    assert_eq!(fetched.json::<Value>().await.unwrap(), patched);

    let invalid_patch = api
        .request(Method::PATCH, &format!("{TASKS_PATH}/{id}"))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body("{")
        .send()
        .await
        .unwrap();
    assert_eq!(invalid_patch.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        invalid_patch.json::<Value>().await.unwrap()["error"],
        "invalid_json"
    );
    let unchanged = api
        .request(Method::GET, &format!("{TASKS_PATH}/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        unchanged.json::<Value>().await.unwrap()["title"],
        "Water herbs"
    );

    let deleted = api
        .request(Method::DELETE, &format!("{TASKS_PATH}/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        deleted.headers()[reqwest::header::CACHE_CONTROL],
        "no-store"
    );
    assert!(
        JsonTaskRepository::new(api.task_path.clone())
            .load()
            .unwrap()
            .is_empty()
    );

    let missing = api
        .request(Method::GET, &format!("{TASKS_PATH}/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        missing.json::<Value>().await.unwrap()["error"],
        "task_not_found"
    );
}

#[tokio::test]
async fn task_api_rejects_non_json_and_invalid_create_bodies() {
    let api = Api::start().await;

    let wrong_media_type = api
        .request(Method::POST, TASKS_PATH)
        .body("{\"title\":\"Task\"}")
        .send()
        .await
        .unwrap();
    assert_eq!(
        wrong_media_type.status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    assert_eq!(
        wrong_media_type.json::<Value>().await.unwrap()["error"],
        "expected_json"
    );

    let invalid_json = api
        .request(Method::POST, TASKS_PATH)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body("not json")
        .send()
        .await
        .unwrap();
    assert_eq!(invalid_json.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        invalid_json.json::<Value>().await.unwrap()["error"],
        "invalid_json"
    );
    assert!(
        JsonTaskRepository::new(api.task_path.clone())
            .load()
            .unwrap()
            .is_empty()
    );
}
