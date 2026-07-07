use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum AuthInfo {
    Api {
        key: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<Value>,
    },
    OAuth {
        refresh: String,
        access: String,
        expires: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        account_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        enterprise_url: Option<String>,
    },
    WellKnown {
        key: String,
        token: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProviderAuthMethod {
    #[serde(rename = "type")]
    pub kind: ProviderAuthMethodKind,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<Vec<ProviderAuthPrompt>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderAuthMethodKind {
    Api,
    OAuth,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ProviderAuthPrompt {
    Text {
        key: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        when: Option<PromptCondition>,
    },
    Select {
        key: String,
        message: String,
        options: Vec<SelectOption>,
        #[serde(skip_serializing_if = "Option::is_none")]
        when: Option<PromptCondition>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PromptCondition {
    pub key: String,
    pub op: PromptConditionOp,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptConditionOp {
    Eq,
    Neq,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProviderAuthAuthorization {
    pub url: String,
    pub method: ProviderAuthAuthorizationMethod,
    pub instructions: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderAuthAuthorizationMethod {
    Auto,
    Code,
}
