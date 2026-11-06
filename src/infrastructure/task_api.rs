use std::{convert::Infallible, sync::Arc, time::Duration};

use http_body_util::{BodyExt, Full, LengthLimitError, Limited};
use hyper::{
    Method, Request, Response, StatusCode,
    body::{Bytes, Incoming},
    header,
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::{TokioIo, TokioTimer};
use serde::{Deserialize, Serialize};
use tokio::sync::{Semaphore, oneshot};
use uuid::Uuid;

use super::task_tokens::TaskTokens;
use crate::{
    application::{
        events::{AppEvent, EventSender},
        todo_service::{TaskCommand, TaskReply, TodoServiceError},
    },
    domain::todo::{TaskColor, TaskSchedule, TodoError},
};

pub const ADDRESS: &str = "127.0.0.1:17642";
const TASKS_PATH: &str = "/v1/tasks";
const MAX_BODY: usize = 16 * 1024;
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn serve(
    listener: tokio::net::TcpListener,
    tokens: Arc<TaskTokens>,
    events: EventSender,
) {
    let host = listener.local_addr().expect("bound listener").to_string();
    let slots = Arc::new(Semaphore::new(16));
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        };
        let Ok(slot) = slots.clone().try_acquire_owned() else {
            continue;
        };
        let (tokens, events, host) = (tokens.clone(), events.clone(), host.clone());
        tokio::spawn(async move {
            let _slot = slot;
            let service = service_fn(move |request| {
                let (tokens, events, host) = (tokens.clone(), events.clone(), host.clone());
                async move { Ok::<_, Infallible>(handle(request, &tokens, &events, &host).await) }
            });
            let mut http = http1::Builder::new();
            http.keep_alive(false)
                .timer(TokioTimer::new())
                .header_read_timeout(Duration::from_secs(5))
                .max_buf_size(MAX_BODY);
            let _ = tokio::time::timeout(
                CONNECTION_TIMEOUT,
                http.serve_connection(TokioIo::new(stream), service),
            )
            .await;
        });
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateTask {
    title: String,
    schedule: Option<TaskSchedule>,
    color: Option<TaskColor>,
}

type ApiResponse = Response<Full<Bytes>>;
type ApiError = (StatusCode, &'static str);

async fn handle(
    request: Request<Incoming>,
    tokens: &TaskTokens,
    events: &EventSender,
    host: &str,
) -> ApiResponse {
    match dispatch(request, tokens, events, host).await {
        Ok(response) => response,
        Err((status, code)) => {
            let mut response = json(status, &serde_json::json!({ "error": code }));
            if status == StatusCode::UNAUTHORIZED {
                response
                    .headers_mut()
                    .insert(header::WWW_AUTHENTICATE, "Bearer".parse().unwrap());
            }
            response
        }
    }
}

async fn dispatch(
    request: Request<Incoming>,
    tokens: &TaskTokens,
    events: &EventSender,
    host: &str,
) -> Result<ApiResponse, ApiError> {
    if request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        != Some(host)
        || request
            .uri()
            .authority()
            .is_some_and(|value| value.as_str() != host)
    {
        return Err((StatusCode::BAD_REQUEST, "invalid_host"));
    }
    let secret = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split_once(' '))
        .filter(|(scheme, secret)| {
            scheme.eq_ignore_ascii_case("Bearer") && tokens.authorizes(secret)
        })
        .map(|(_, secret)| secret.to_owned())
        .ok_or((StatusCode::UNAUTHORIZED, "unauthorized"))?;

    let path = request.uri().path();
    let id = if path == TASKS_PATH {
        None
    } else if let Some(id) = path
        .strip_prefix("/v1/tasks/")
        .filter(|id| !id.contains('/'))
    {
        Some(Uuid::parse_str(id).map_err(|_| (StatusCode::BAD_REQUEST, "invalid_id"))?)
    } else {
        return Err((StatusCode::NOT_FOUND, "not_found"));
    };
    let mut success_status = StatusCode::OK;
    let command = match (request.method(), id) {
        (&Method::GET, None) => TaskCommand::List,
        (&Method::GET, Some(id)) => TaskCommand::Get(id),
        (&Method::POST, None) => {
            let body: CreateTask = read_json(request).await?;
            success_status = StatusCode::CREATED;
            TaskCommand::Create {
                title: body.title,
                schedule: body.schedule,
                color: body.color,
            }
        }
        (&Method::PATCH, Some(id)) => TaskCommand::Update(id, read_json(request).await?),
        (&Method::DELETE, Some(id)) => TaskCommand::Delete(id),
        (_, id) => {
            let mut response = json(
                StatusCode::METHOD_NOT_ALLOWED,
                &serde_json::json!({ "error": "method_not_allowed" }),
            );
            response.headers_mut().insert(
                header::ALLOW,
                if id.is_some() {
                    "GET, PATCH, DELETE"
                } else {
                    "GET, POST"
                }
                .parse()
                .unwrap(),
            );
            return Ok(response);
        }
    };
    if !tokens.authorizes(&secret) {
        return Err((StatusCode::UNAUTHORIZED, "unauthorized"));
    }
    let (reply, result) = oneshot::channel();
    let unavailable = (StatusCode::SERVICE_UNAVAILABLE, "tasks_unavailable");
    events
        .send(AppEvent::TaskApi { command, reply })
        .map_err(|_| unavailable)?;
    match result.await.map_err(|_| unavailable)? {
        Ok(TaskReply::List(tasks)) => Ok(json(success_status, &tasks)),
        Ok(TaskReply::Task(task)) => {
            let mut response = json(success_status, &task);
            if success_status == StatusCode::CREATED {
                response.headers_mut().insert(
                    header::LOCATION,
                    format!("{TASKS_PATH}/{}", task.id).parse().unwrap(),
                );
            }
            Ok(response)
        }
        Ok(TaskReply::Deleted) => Ok(Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header(header::CACHE_CONTROL, "no-store")
            .body(Full::default())
            .unwrap()),
        Err(TodoServiceError::Rule(TodoError::NotFound)) => {
            Err((StatusCode::NOT_FOUND, "task_not_found"))
        }
        Err(TodoServiceError::Rule(error)) => Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            match error {
                TodoError::EmptyTitle => "empty_title",
                TodoError::InvalidSchedule => "invalid_schedule",
                TodoError::InvalidColor => "invalid_color",
                TodoError::NotFound => unreachable!(),
            },
        )),
        Err(TodoServiceError::Storage(_)) => {
            Err((StatusCode::INTERNAL_SERVER_ERROR, "storage_error"))
        }
    }
}

async fn read_json<T: serde::de::DeserializeOwned>(
    request: Request<Incoming>,
) -> Result<T, ApiError> {
    let media_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or_default().trim());
    if !media_type.is_some_and(|value| value.eq_ignore_ascii_case("application/json")) {
        return Err((StatusCode::UNSUPPORTED_MEDIA_TYPE, "expected_json"));
    }
    let bytes = Limited::new(request.into_body(), MAX_BODY)
        .collect()
        .await
        .map_err(|error| {
            if error.is::<LengthLimitError>() {
                (StatusCode::PAYLOAD_TOO_LARGE, "body_too_large")
            } else {
                (StatusCode::BAD_REQUEST, "invalid_body")
            }
        })?
        .to_bytes();
    serde_json::from_slice(&bytes).map_err(|_| (StatusCode::BAD_REQUEST, "invalid_json"))
}

fn json(status: StatusCode, value: &impl Serialize) -> ApiResponse {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Full::new(Bytes::from(
            serde_json::to_vec(value).expect("serializable API response"),
        )))
        .unwrap()
}

#[cfg(test)]
#[path = "../../tests/fixtures/task_api.rs"]
mod tests;
