use serde::Deserialize;

#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
pub struct WitMessageResponse {
    pub text: String,
    pub intents: Vec<Intent>,
    pub entities: Entities,
    pub traits: Traits,
}

#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
pub struct Intent {
    pub name: String,
}

#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
pub struct Entities {
    #[serde(rename = "account:destination")]
    pub destination: Vec<AccountEntity>,

    #[serde(rename = "account:origin")]
    pub origin: Vec<AccountEntity>,

    #[serde(rename = "wit$amount_of_money:amount_of_money")]
    pub amount_of_money: Vec<WitAmountOfMoney>,

    #[serde(default)]
    #[serde(rename = "action:withdraw")]
    pub withdraw: Option<Vec<ActionEntity>>,

    #[serde(default)]
    #[serde(rename = "action:deposit")]
    pub deposit: Option<Vec<ActionEntity>>,

    #[serde(default)]
    #[serde(rename = "action:transfer")]
    pub transfer: Option<Vec<ActionEntity>>,

    #[serde(default)]
    #[serde(rename = "deed:deed")]
    pub deed: Option<Vec<Deed>>,
}

#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
pub struct AccountEntity {
    pub role: String,
    pub value: String,
}

#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
pub struct WitAmountOfMoney {
    pub role: String,
    pub unit: String,
    pub value: f64,
}

#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
pub struct ActionEntity {
    pub role: String,
}

#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
pub struct Deed {
    pub role: String,
    pub value: String,
}

#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
pub struct Traits {
    pub flow: Vec<Flow>,
}

#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
pub struct Flow {
    pub value: String,
}
