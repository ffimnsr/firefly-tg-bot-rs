use std::env;
use log::{info, error};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use routerify::{Middleware, Router, RouterService};
use serde::Deserialize;
use lazy_static::lazy_static;

pub type GenericError = Box<dyn std::error::Error + Send + Sync>;

pub type ServiceResult<T> = std::result::Result<T, GenericError>;

/// This object represents a chat.
#[derive(Debug, Deserialize)]
struct Chat {
    /// Unique identifier for this chat. This number may have more than 32 significant bits and some programming languages may have difficulty/silent defects in interpreting it. But it has at most 52 significant bits, so a signed 64-bit integer or double-precision float type are safe for storing this identifier.
    pub id: i32,

    /// Type of chat, can be either “private”, “group”, “supergroup” or “channel”.
    #[serde(rename = "type")]
    pub chat_type: String,

    /// Title, for supergroups, channels and group chats.
    pub title: Option<String>,
}

/// This object represents a message.
#[derive(Debug, Deserialize)]
struct Message {
    /// Unique message identifier inside this chat
    pub message_id: i32,

    /// Date the message was sent in Unix time
    pub date: i64,

    /// For text messages, the actual UTF-8 text of the message, 0-4096 characters.
    pub text: Option<String>,
}

const JSON_MIME: &str = "application/json";

lazy_static! {
    static ref TG_BOT_TOKEN: String = {
        env::var("TG_BOT_TOKEN").expect("Telegram bot token not set.")
    };
}

async fn hello_world(_: Request<Body>) -> ServiceResult<Response<Body>> {
    let data = serde_json::json!({
        "success": true,
        "message": "How long is forever?",
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(
            hyper::header::CONTENT_TYPE,
            JSON_MIME,
        )
        .body(Body::from(data.to_string()))?)
}

async fn handle_telegram_message(_: Request<Body>) -> ServiceResult<Response<Body>> {
    let url = format!("https://https://api.telegram.org/bot{}/{}", *TG_BOT_TOKEN, "sendMessage");
    let tg_resp = reqwest::Client::new()
        .post(&url)
        .json(&serde_json::json!({
            "hello": 1,
        }))
        .send()
        .await?;
        
    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(Body::empty())?)
}

async fn handler_404(req: Request<Body>) -> ServiceResult<Response<Body>> {
    match *req.method() {
        // To handle cors options request.
        // Needed similar to https://github.com/expressjs/cors/blob/c49ca10e92ac07f98a3b06783d3e6ba0ea5b70c7/lib/index.js#L173
        Method::OPTIONS => Ok(Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header(hyper::header::CONTENT_LENGTH, "0")
            .body(Body::empty())?),
        _ => {
            let data = serde_json::json!({
                "success": false,
                "message": "Not Found",
            });

            Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header(
                    hyper::header::CONTENT_TYPE,
                    "application/json; charset=utf-8",
                )
                .body(Body::from(data.to_string()))?)
        }
    }
}

fn router() -> ServiceResult<Router<Body, GenericError>> {
    Router::builder()
        .middleware(Middleware::pre(|req: Request<Body>| async move {
            let (parts, body) = req.into_parts();
            let body_raw = hyper::body::to_bytes(body).await?;

            let cloned_body_raw = body_raw.clone();
            if cloned_body_raw.is_empty() {    
                info!(
                    "REQ {:?} {} {}",
                    parts.version,
                    parts.method,
                    parts.uri.path()
                );
    
                let request = Request::from_parts(parts, Body::empty());
                Ok(request)
            } else {
                let body_str = String::from_utf8_lossy(cloned_body_raw.as_ref());
                let json_value: serde_json::Value = serde_json::from_str(body_str.to_string().as_str())?;
    
                info!(
                    "REQ {:?} {} {} <= {:?}",
                    parts.version,
                    parts.method,
                    parts.uri.path(),
                    serde_json::to_string_pretty(&json_value)?,
                );
    
                let request = Request::from_parts(parts, Body::from(body_raw));
                Ok(request)
            }
        }))
        .middleware(Middleware::post(|res: Response<Body>| async move {
            let (parts, body) = res.into_parts();
            let body_raw = hyper::body::to_bytes(body).await?;

            let cloned_body_raw = body_raw.clone();
            let body_str = String::from_utf8_lossy(cloned_body_raw.as_ref());
            let json_value: serde_json::Value = serde_json::from_str(body_str.to_string().as_str())?;

            info!(
                "RES {:?} =>\n{}",
                parts.status,
                serde_json::to_string_pretty(&json_value)?,
            );

            let response = Response::from_parts(parts, Body::from(body_raw));
            Ok(response)
        }))
        .get("/", hello_world)
        .post("/hook", handle_telegram_message)
        .any(handler_404)
        .build()
}

#[tokio::main]
async fn main() -> Result<(), GenericError> {
    dotenv::dotenv().ok();

    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "firefly_tg=trace");
        env::set_var("RUST_BACKTRACE", "1");
    }

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let router = router()?;
    let service = RouterService::new(router)?;

    let default_port = Some(80u16);
    let port =  env::var("PORT")
        .ok()
        .and_then(|n| n.parse::<u16>().ok())
        .or(default_port).unwrap();

    let addr = ([0, 0, 0, 0], port).into();
    info!("Firefly telegram bot service is now listening at {}", addr);

    let server = Server::bind(&addr).serve(service);

    if let Err(e) = server.await {
        error!("A server error occurred: {}", e);
    }

    Ok(())
}
