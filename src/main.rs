mod telegram;

use std::{env, sync::Arc};
use log::{info, error};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use routerify::prelude::*;
use routerify::{Middleware, Router, RouterService};
use serde::Deserialize;
use lazy_static::lazy_static;
use sled_extensions::DbExt;
use sled_extensions::bincode::Tree;
use telegram::{TelegramContext, UserClue};

pub type GenericError = Box<dyn std::error::Error + Send + Sync>;

pub type ServiceResult<T> = std::result::Result<T, GenericError>;

/// This object represents a Telegram user or bot.
#[derive(Debug, Deserialize)]
pub struct User {
    /// Unique identifier for this user or bot. This number may have more than 32 significant bits and some programming languages may have difficulty/silent defects in interpreting it. But it has at most 52 significant bits, so a 64-bit integer or double-precision float type are safe for storing this identifier.
    pub id: i32,

    /// True, if this user is a bot
    pub is_bot: bool,

    /// User's or bot's first name
    pub first_name: String,

    /// User's or bot's last name
    pub last_name: Option<String>,

    /// User's or bot's username
    pub username: Option<String>,
}

/// This object represents a chat.
#[derive(Debug, Deserialize)]
pub struct Chat {
    /// Unique identifier for this chat. This number may have more than 32 significant bits and some programming languages may have difficulty/silent defects in interpreting it. But it has at most 52 significant bits, so a signed 64-bit integer or double-precision float type are safe for storing this identifier.
    pub id: i32,

    /// Type of chat, can be either “private”, “group”, “supergroup” or “channel”.
    #[serde(rename = "type")]
    pub chat_type: String,

    /// Username, for private chats, supergroups and channels if available
    pub username: Option<String>,

    /// First name of the other party in a private chat
    pub first_name: Option<String>,

    /// Last name of the other party in a private chat
    pub last_name: Option<String>,
}

/// This object represents a message.
#[derive(Debug, Deserialize)]
pub struct Message {
    /// Unique message identifier inside this chat
    pub message_id: i32,

    /// Date the message was sent in Unix time
    pub date: i64,

    /// For text messages, the actual UTF-8 text of the message, 0-4096 characters.
    pub text: Option<String>,

    /// Conversation the message belongs to
    pub chat: Chat,

    /// Sender, empty for messages sent to channels
    pub from: Option<User>,
}

/// This object represents an incoming update.
#[derive(Debug, Deserialize)]
pub struct Update {
    /// The update's unique identifier. Update identifiers start from a certain positive number and increase sequentially. This ID becomes especially handy if you're using Webhooks, since it allows you to ignore repeated updates or to restore the correct update sequence, should they get out of order. If there are no new updates for at least a week, then identifier of the next update will be chosen randomly instead of sequentially.
    pub update_id: i32,

    /// New incoming message of any kind -- text, photo, sticker, etc.
    pub message: Option<Message>,
}

pub struct Database {
    users: Tree<UserClue>,
}

const JSON_MIME: &str = "application/json";
const VERSION: &str = env!("CARGO_PKG_VERSION");

lazy_static! {
    static ref TG_BOT_TOKEN: String = {
        env::var("TG_BOT_TOKEN").expect("Telegram bot token not set.")
    };
    static ref TG_MASTER_ID: String = {
        env::var("TG_MASTER_ID").expect("Telegram master id not set.")
    };
    static ref APP_SHARED_STORAGE_PATH: String = {
        env::var("APP_SHARED_STORAGE_PATH").expect("App shared storage not set.")
    };
}

async fn hello_world(_: Request<Body>) -> ServiceResult<Response<Body>> {
    let data = serde_json::json!({
        "success": true,
        "message": "How long is forever?",
        "version": VERSION,
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(
            hyper::header::CONTENT_TYPE,
            JSON_MIME,
        )
        .body(Body::from(data.to_string()))?)
}

async fn handle_telegram_message(req: Request<Body>) -> ServiceResult<Response<Body>> {
    let db = req.data::<Arc<Database>>().ok_or("Unknown KV database instance")?.to_owned();
    let (_, body) = req.into_parts();
    let body_raw = hyper::body::to_bytes(body).await?;
    let update = serde_json::from_slice::<Update>(&body_raw)?;
    let mut context = TelegramContext::new(db.to_owned());
    let tg_resp = context.process_message(update).await;

    match tg_resp {
        Ok(t) => {
            if t.status() == StatusCode::OK {
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header(hyper::header::CONTENT_LENGTH, 0)
                    .body(Body::empty())?)
            } else {
                let details: serde_json::Value = serde_json::from_slice(&t.bytes().await.unwrap())?;
                let data = serde_json::json!({
                    "success": false,
                    "message": "An unknown error occurred in the bot kindly check the logs for more info.",
                    "details": details,
                });

                Ok(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header(
                        hyper::header::CONTENT_TYPE,
                        JSON_MIME,
                    )
                    .body(Body::from(data.to_string()))?)
            }
        },
        Err(e) => {
            send_report(&e.to_string()).await;

            let data = serde_json::json!({
                "success": false,
                "message": "An error occurred in the bot kindly check the logs for more info.",
                "details": e.to_string(),
            });

            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header(
                    hyper::header::CONTENT_TYPE,
                    JSON_MIME,
                )
                .body(Body::from(data.to_string()))?)
        },
    }
}

async fn send_report(error_message: &str) {
    let message = format!("Firefly Bot Error: {}", error_message);

    let tg_resp = telegram_post("sendMessage", &serde_json::json!({
        "chat_id": *TG_MASTER_ID,
        "text": message,
    }))
    .await;

    tg_resp.expect("Failed to communicate with Telegram servers");
}

pub async fn telegram_post(endpoint: &str, payload: &serde_json::Value) -> Result<reqwest::Response, reqwest::Error> {
    let url = format!("https://api.telegram.org/bot{}/{}", *TG_BOT_TOKEN, endpoint);

    reqwest::Client::new()
        .post(&url)
        .json(payload)
        .send()
        .await
}

async fn handler_404(req: Request<Body>) -> ServiceResult<Response<Body>> {
    match *req.method() {
        // To handle cors options request.
        // Needed similar to https://github.com/expressjs/cors/blob/c49ca10e92ac07f98a3b06783d3e6ba0ea5b70c7/lib/index.js#L173
        Method::OPTIONS => Ok(Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header(hyper::header::CONTENT_LENGTH, 0)
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
                    "application/json",
                )
                .body(Body::from(data.to_string()))?)
        }
    }
}

fn router() -> ServiceResult<Router<Body, GenericError>> {
    let db = sled_extensions::Config::default()
        .path(&*APP_SHARED_STORAGE_PATH)
        .open()?;

    Router::builder()
        .middleware(Middleware::pre(|req: Request<Body>| async move {
            let (parts, body) = req.into_parts();
            let body_raw = hyper::body::to_bytes(body).await?;

            if body_raw.is_empty() {
                info!(
                    "REQ {:?} {} {}",
                    parts.version,
                    parts.method,
                    parts.uri.path()
                );

                let request = Request::from_parts(parts, Body::empty());
                Ok(request)
            } else {
                let cloned_body_raw = body_raw.clone();
                let json_value: serde_json::Value = serde_json::from_slice(&cloned_body_raw)?;

                info!(
                    "REQ {:?} {} {} <=\n{}",
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

            if body_raw.is_empty() {
                info!("RES {:?}", parts.status);

                let response = Response::from_parts(parts, Body::empty());
                Ok(response)
            } else {
                let cloned_body_raw = body_raw.clone();
                let json_value: serde_json::Value = serde_json::from_slice(&cloned_body_raw)?;

                info!(
                    "RES {:?} =>\n{}",
                    parts.status,
                    serde_json::to_string_pretty(&json_value)?,
                );

                let response = Response::from_parts(parts, Body::from(body_raw));
                Ok(response)
            }
        }))
        .data(Arc::new(Database {
            users: db.open_bincode_tree("users")?,
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
