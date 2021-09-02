mod telegram;
mod wit;

use std::{env, sync::Arc};
use log::{info, error};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use routerify::prelude::*;
use routerify::{Middleware, Router, RouterService};
use lazy_static::lazy_static;
use sled_extensions::DbExt;
use sled_extensions::bincode::Tree;
use telegram::{TelegramContext, UserClue};

pub type GenericError = Box<dyn std::error::Error + Send + Sync>;

pub type ServiceResult<T> = std::result::Result<T, GenericError>;

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
    static ref WIT_ACCESS_TOKEN: String = {
        env::var("WIT_ACCESS_TOKEN").expect("Wit access token not set.")
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

async fn run_expensive_task(db: Arc<Database>, update: telegram::Update) -> ServiceResult<()> {
    let mut context = TelegramContext::new(db.to_owned());
    let tg_resp = context.process_message(update).await;
    match tg_resp {
        Ok(t) => {
            if t.status() != StatusCode::OK {
                let details: serde_json::Value = serde_json::from_slice(&t.bytes().await.unwrap())?;
                let data = serde_json::json!({
                    "success": false,
                    "message": "An unknown error occurred in the bot kindly check the logs for more info.",
                    "details": details,
                });

                error!("Fatal error occurred:\n{}", serde_json::to_string_pretty(&data)?);
            }
        },
        Err(e) => {
            send_report(&e.to_string()).await;

            let data = serde_json::json!({
                "success": false,
                "message": "An error occurred in the bot kindly check the logs for more info.",
                "details": e.to_string(),
            });

            error!("Fatal error occurred:\n{}", serde_json::to_string_pretty(&data)?);
        },
    }

    Ok(())
}

async fn handle_telegram_message(req: Request<Body>) -> ServiceResult<Response<Body>> {
    let db = req.data::<Arc<Database>>().ok_or("Unknown key-value store instance")?.to_owned();
    let (_, body) = req.into_parts();
    let body_raw = hyper::body::to_bytes(body).await?;
    let update = serde_json::from_slice::<telegram::Update>(&body_raw)?;

    tokio::spawn(run_expensive_task(db, update));

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(hyper::header::CONTENT_LENGTH, 0)
        .body(Body::empty())?)
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

pub async fn wit_message_get(query: &str) -> Result<reqwest::Response, reqwest::Error> {
    reqwest::Client::new()
        .get("https://api.wit.ai/message")
        .query(&[("v", "20210902"), ("q", query)])
        .bearer_auth(&*WIT_ACCESS_TOKEN)
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
