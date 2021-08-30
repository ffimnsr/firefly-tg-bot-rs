use std::sync::Arc;
use serde::{Deserialize, Serialize};
use chrono::Utc;
use log::info;

use super::{Database, GenericError, Update};

#[derive(Clone, Default)]
pub struct State {
    from_id: i32,
    chat_id: i32,
}

impl State {
    pub fn user_id(&self) -> String {
        format!("telegram-user-{}", self.from_id)
    }
}

pub struct TelegramContext {
    db: Arc<Database>,
    state: Arc<State>,
}

impl TelegramContext {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            state: Arc::new(Default::default()),
        }
    }

    pub fn set_state(&mut self, new_state: State) {
        self.state = Arc::new(new_state);
    }

    pub fn get_user_id(&self) -> Vec<u8> {
        self.state.user_id().as_bytes().to_owned()
    }

    pub async fn process_message(&mut self, update: Update) -> Result<reqwest::Response, GenericError> {
        let message = update.message.ok_or("No message")?;
        let text_payload = message.text.ok_or("Empty text payload")?;
        let chat = message.chat;

        let from_id = message.from.ok_or("No user from included in payload")?.id;
        self.set_state(State {
            from_id,
            chat_id: chat.id,
        });

        match text_payload.as_str() {
            "/start" => self.cmd_start().await,
            "/reset" => self.cmd_reset().await,
            "/help" => self.cmd_help().await,
            "/test" => self.cmd_test().await,
            _ => self.cmd_transact(&text_payload).await,
        }
    }

    async fn cmd_start(&self) -> Result<reqwest::Response, GenericError> {
        self.db.users.insert(self.get_user_id(), UserClue::new(self.state.from_id))?;

        let tg_resp = super::telegram_post("sendMessage", &serde_json::json!({
            "chat_id": self.state.chat_id,
            "parse_mode": "Markdown",
            "text": "Please enter your *Firefly III* server's URL (e.g. https://my-firefly-iii.com).\n\nIt must start with HTTP/s protocol scheme.",
        }))
        .await
        .map_err(|e| e.into());

        tg_resp
    }

    async fn cmd_reset(&self) -> Result<reqwest::Response, GenericError> {
        self.db.users.remove(self.get_user_id())?;

        let tg_resp = super::telegram_post("sendMessage", &serde_json::json!({
            "chat_id": self.state.chat_id,
            "text": "Reset complete.",
        }))
        .await
        .map_err(|e| e.into());

        tg_resp
    }

    async fn cmd_help(&self) -> Result<reqwest::Response, GenericError> {
        let is_exists = self.db.users.contains_key(self.get_user_id())?;

        if !is_exists {
            let tg_resp = super::telegram_post("sendMessage", &serde_json::json!({
                "chat_id": self.state.chat_id,
                "text": "Type /start to initiate the setup process.",
            }))
            .await
            .map_err(|e| e.into());

            tg_resp
        } else {
            let tg_resp = super::telegram_post("sendMessage", &serde_json::json!({
                "chat_id": self.state.chat_id,
                "parse_mode": "Markdown",
                "text": "
                Send a message in the following format \
                \n`Amount, Description, Source, Destination` \
                \n\nThe first two parameters are required.
                ",
            }))
            .await
            .map_err(|e| e.into());

            tg_resp
        }
    }

    async fn cmd_test(&self) -> Result<reqwest::Response, GenericError> {
        let tg_resp = super::telegram_post("sendMessage", &serde_json::json!({
            "chat_id": self.state.chat_id,
            "text": "Message Ack",
        }))
        .await
        .map_err(|e| e.into());

        tg_resp
    }

    async fn cmd_transact(&self, payload: &str) -> Result<reqwest::Response, GenericError> {
        let exist = self.db.users.get(self.get_user_id())?;

        if let Some(user) = exist {
            if user.is_ready() {
                self.transact(user, payload).await
            } else {
                match user.state.as_str() {
                    "upload-url" => self.upload_url(payload).await,
                    "upload-pat" => self.upload_pat(payload).await,
                    _ => Err("Unknown user state".into()),
                }
            }
        } else {
            let tg_resp = super::telegram_post("sendMessage", &serde_json::json!({
                "chat_id": self.state.chat_id,
                "text": "Type /start to initiate the setup process.",
            }))
            .await
            .map_err(|e| e.into());

            tg_resp
        }
    }

    async fn transact(&self, user: UserClue, payload: &str) -> Result<reqwest::Response, GenericError> {
        let params = payload.split(',').map(|x| x.trim().to_owned()).collect::<Vec<String>>();
        let today = Utc::now();

        let transact = Transaction {
            transact_type: "withdrawal".into(),
            amount: params[0].clone(),
            description: params[1].clone(),
            date: today.format("%Y-%m-%d").to_string(),
            source_name: params[2].clone(),
            destination_name: params[3].clone(),
        };

        info!("{:?}", transact);

        user.create_transaction(TransactPayload { transactions: vec![transact] }).await?;

        let tg_resp = super::telegram_post("sendMessage", &serde_json::json!({
            "chat_id": self.state.chat_id,
            "text": "Transaction created.",
        }))
        .await
        .map_err(|e| e.into());

        tg_resp
    }

    async fn upload_url(&self, payload: &str) -> Result<reqwest::Response, GenericError> {
        let firefly_url = payload.trim();

        let mut user = self.db.users.get(self.get_user_id())?.ok_or("Cannot find the user in the database")?;
        user.firefly_url = firefly_url.to_owned();
        user.state = "upload-pat".into();
        self.db.users.insert(self.get_user_id(), user)?;

        let message = format!("Your *Firefly III* URL's been saved!\n\nNow please enter your firefly *Personal Access Token* (PAT), you can generate it from PAT section here - {}/profile", firefly_url);
        let tg_resp = super::telegram_post("sendMessage", &serde_json::json!({
            "chat_id": self.state.chat_id,
            "parse_mode": "Markdown",
            "text": message,
        }))
        .await
        .map_err(|e| e.into());

        tg_resp
    }

    async fn upload_pat(&self, payload: &str) -> Result<reqwest::Response, GenericError> {
        let firefly_pat = payload.trim();

        let mut user = self.db.users.get(self.get_user_id())?.ok_or("Cannot find the user in the database")?;
        user.firefly_pat = firefly_pat.to_owned();
        user.state = "ready".into();
        self.db.users.insert(self.get_user_id(), user)?;

        let tg_resp = super::telegram_post("sendMessage", &serde_json::json!({
            "chat_id": self.state.chat_id,
            "text": "Setup complete. You can now use the telegram bot to store your transaction.",
        }))
        .await
        .map_err(|e| e.into());

        tg_resp
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TransactPayload {
    transactions: Vec<Transaction>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Transaction {
    #[serde(rename = "type")]
    transact_type: String,
    description: String,
    date: String,
    amount: String,
    source_name: String,
    destination_name: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct UserClue {
    id: i32,
    state: String,
    firefly_url: String,
    firefly_pat: String,
}

impl UserClue {
    pub fn new(id: i32) -> Self {
        Self {
            id,
            state: "upload-url".into(),
            ..Default::default()
        }
    }

    pub fn is_ready(&self) -> bool {
        self.state.eq("ready".into())
    }

    #[allow(unused)]
    async fn get_accounts(&self, account_type: &str) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}/public/api/v1/accounts", self.firefly_url.to_owned());

        reqwest::Client::new()
            .get(&url)
            .query(&[("type", account_type)])
            .bearer_auth(self.firefly_pat.to_owned())
            .send()
            .await
    }

    async fn create_transaction(&self, payload: TransactPayload) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}/public/api/v1/transactions", self.firefly_url.to_owned());

        reqwest::Client::new()
            .post(&url)
            .json(&payload)
            .bearer_auth(self.firefly_pat.to_owned())
            .send()
            .await
    }
}
