use std::collections::BTreeMap;

use neoism_agent_core::{
    AuthInfo, ProviderAuthAuthorization, ProviderAuthAuthorizationMethod,
    ProviderAuthMethod, ProviderAuthMethodKind, ProviderInfo,
};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::auth_store::AuthStore;
use crate::state::ProviderOAuthPending;

#[path = "provider_auth_copilot.rs"]
mod provider_auth_copilot;
#[path = "provider_auth_methods.rs"]
mod provider_auth_methods;
#[path = "provider_auth_openai.rs"]
mod provider_auth_openai;
#[path = "provider_auth_util.rs"]
mod provider_auth_util;

#[cfg(test)]
use crate::provider_auth_browser::openai_browser_callback_outcome;
use provider_auth_copilot::{authorize_github_copilot, poll_github_copilot};
use provider_auth_methods::{provider_metadata, provider_methods, select_method};
use provider_auth_openai::{
    authorize_openai_browser, authorize_openai_headless, exchange_openai_browser,
    poll_openai_headless,
};

pub(crate) fn methods(
    providers: &[ProviderInfo],
) -> BTreeMap<String, Vec<ProviderAuthMethod>> {
    providers
        .iter()
        .map(|provider| (provider.id.clone(), provider_methods(provider)))
        .collect()
}

pub(crate) async fn authorize(
    provider_id: &str,
    method: &Value,
    inputs: &BTreeMap<String, String>,
    providers: &[ProviderInfo],
    auth_store: &AuthStore,
    pending: &RwLock<std::collections::HashMap<String, ProviderOAuthPending>>,
) -> anyhow::Result<Option<ProviderAuthAuthorization>> {
    let method = select_method(provider_id, method, providers)?;
    match method.kind {
        ProviderAuthMethodKind::Api => {
            if let Some(key) = inputs.get("key").filter(|key| !key.trim().is_empty()) {
                auth_store.set(
                    provider_id,
                    AuthInfo::Api {
                        key: key.clone(),
                        metadata: provider_metadata(inputs),
                    },
                )?;
            }
            Ok(None)
        }
        ProviderAuthMethodKind::OAuth => {
            if provider_id == "openai" && method.label.contains("browser") {
                return authorize_openai_browser(provider_id, pending)
                    .await
                    .map(Some);
            }
            if provider_id == "openai" && method.label.contains("headless") {
                return authorize_openai_headless(provider_id, pending)
                    .await
                    .map(Some);
            }
            if provider_id.starts_with("github-copilot") {
                return authorize_github_copilot(provider_id, inputs, pending)
                    .await
                    .map(Some);
            }
            Ok(Some(ProviderAuthAuthorization {
                url: format!("neoism://provider/{provider_id}/oauth/manual"),
                method: ProviderAuthAuthorizationMethod::Code,
                instructions: format!(
                    "Paste an OAuth access token for {provider_id}. Neoism will store it as both access and refresh token unless a provider-specific OAuth flow is added."
                ),
            }))
        }
    }
}

pub(crate) async fn callback(
    provider_id: &str,
    method: &Value,
    code: Option<&str>,
    providers: &[ProviderInfo],
    auth_store: &AuthStore,
    pending: &RwLock<std::collections::HashMap<String, ProviderOAuthPending>>,
) -> anyhow::Result<()> {
    let method = select_method(provider_id, method, providers)?;
    match method.kind {
        ProviderAuthMethodKind::Api => {
            let Some(key) = code.filter(|code| !code.trim().is_empty()) else {
                anyhow::bail!("API key is required for provider {provider_id}")
            };
            auth_store.set(
                provider_id,
                AuthInfo::Api {
                    key: key.to_string(),
                    metadata: None,
                },
            )?;
            Ok(())
        }
        ProviderAuthMethodKind::OAuth => {
            if provider_id == "openai" && method.label.contains("browser") {
                let auth = exchange_openai_browser(provider_id, pending).await?;
                auth_store.set(provider_id, auth)?;
                return Ok(());
            }
            if provider_id == "openai" && method.label.contains("headless") {
                let auth = poll_openai_headless(provider_id, pending).await?;
                auth_store.set(provider_id, auth)?;
                return Ok(());
            }
            if provider_id.starts_with("github-copilot") {
                let auth = poll_github_copilot(provider_id, pending).await?;
                auth_store.set(provider_id, auth)?;
                return Ok(());
            }
            if let Some(code) = code.filter(|code| !code.trim().is_empty()) {
                auth_store.set(
                    provider_id,
                    AuthInfo::OAuth {
                        refresh: code.to_string(),
                        access: code.to_string(),
                        expires: 0,
                        account_id: None,
                        enterprise_url: None,
                    },
                )?;
                return Ok(());
            }
            anyhow::bail!("OAuth token is required for provider {provider_id}")
        }
    }
}

#[cfg(test)]
#[path = "provider_auth_tests.rs"]
mod tests;
