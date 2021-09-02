use std::sync::Arc;
use serde::{Deserialize, Serialize};
use chrono::Utc;
use tokio::time::{sleep, Duration};

use crate::wit::{Deed, WitMessageResponse};

use super::{Database, GenericError};

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

        super::telegram_post("sendChatAction", &serde_json::json!({
            "chat_id": self.state.chat_id,
            "action": "typing",
        })).await?;

        sleep(Duration::from_secs(5)).await;

        match text_payload.as_str() {
            "/start" => self.cmd_start().await,
            "/reset" => self.cmd_reset().await,
            "/help" => self.cmd_help().await,
            "/test" => self.cmd_test().await,
            _ => self.cmd_transact(&text_payload).await,
        }
    }

    async fn cmd_start(&self) -> Result<reqwest::Response, GenericError> {
        let exists = self.db.users.contains_key(self.get_user_id())?;

        if exists {
            let tg_resp = super::telegram_post("sendMessage", &serde_json::json!({
                "chat_id": self.state.chat_id,
                "text": "Type /reset to reset your account.",
            }))
            .await
            .map_err(|e| e.into());

            tg_resp
        } else {
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
                \n`The deed. And the transaction.`
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
        let wit_response = super::wit_message_get(payload)
            .await?
            .json::<WitMessageResponse>()
            .await?;

        if wit_response.intents.len().gt(&0) {
            let description = wit_response.entities.deed
                .unwrap_or(vec![])
                .get(0)
                .unwrap_or(&Deed {
                    value: wit_response.text,
                    ..Default::default()
                })
                .value
                .to_owned();
            let amount = wit_response.entities.amount_of_money
                .get(0)
                .ok_or("The amount of money is empty.")?
                .value
                .to_string();
            let source_name = wit_response.entities.origin
                .get(0)
                .ok_or("The account origin is empty.")?
                .value
                .to_owned();
            let destination_name = wit_response.entities.destination
                .get(0)
                .ok_or("The account destination is empty.")?
                .value
                .to_owned();
            let transact_type = wit_response.traits.flow
                .get(0)
                .ok_or("The transact type is empty.")?
                .value
                .to_owned();

            let transact = Transaction {
                transact_type,
                amount,
                description,
                source_name,
                destination_name,
                date: Utc::now().format("%Y-%m-%d").to_string(),
            };

            user.create_transaction(TransactPayload { transactions: vec![transact] }).await?;

            log::info!("Transaction created");

            let tg_resp = super::telegram_post("sendMessage", &serde_json::json!({
                "chat_id": self.state.chat_id,
                "text": "Transaction created.",
            }))
            .await
            .map_err(|e| e.into());

            tg_resp
        } else {
            let tg_resp = super::telegram_post("sendMessage", &serde_json::json!({
                "chat_id": self.state.chat_id,
                "text": "Type /help to check the proper way of creating a transaction.",
            }))
            .await
            .map_err(|e| e.into());

            tg_resp
        }
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
