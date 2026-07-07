use std::time::Duration;

use neoism_agent_core::McpOAuthConfig;
use reqwest::header::ACCEPT;
use serde::de::DeserializeOwned;
use serde::Deserialize;

#[derive(Clone, Debug)]
pub(super) struct OAuthEndpoints {
    pub(super) authorization_url: String,
    pub(super) token_url: String,
    pub(super) registration_url: Option<String>,
}

pub(super) async fn oauth_endpoints(
    server_url: &str,
    oauth: &McpOAuthConfig,
    needs_authorization: bool,
    needs_token: bool,
    needs_registration: bool,
) -> OAuthEndpoints {
    let should_discover = (needs_authorization && oauth.authorization_url.is_none())
        || (needs_token && oauth.token_url.is_none())
        || (needs_registration && oauth.registration_url.is_none());
    let metadata = if should_discover {
        discover_oauth_metadata(server_url).await
    } else {
        None
    };
    let origin = origin(server_url).unwrap_or_else(|| server_url.to_string());
    OAuthEndpoints {
        authorization_url: oauth
            .authorization_url
            .clone()
            .or_else(|| {
                metadata
                    .as_ref()
                    .and_then(|metadata| metadata.authorization_endpoint.clone())
            })
            .unwrap_or_else(|| format!("{origin}/authorize")),
        token_url: oauth
            .token_url
            .clone()
            .or_else(|| {
                metadata
                    .as_ref()
                    .and_then(|metadata| metadata.token_endpoint.clone())
            })
            .unwrap_or_else(|| format!("{origin}/token")),
        registration_url: oauth.registration_url.clone().or_else(|| {
            metadata
                .as_ref()
                .and_then(|metadata| metadata.registration_endpoint.clone())
        }),
    }
}

async fn discover_oauth_metadata(server_url: &str) -> Option<OAuthServerMetadata> {
    let client = oauth_discovery_client();
    for protected_url in protected_resource_metadata_urls(server_url) {
        let Some(protected) =
            get_json::<OAuthProtectedResourceMetadata>(&client, &protected_url).await
        else {
            continue;
        };
        for issuer in protected.authorization_servers.unwrap_or_default() {
            for metadata_url in authorization_server_metadata_urls(&issuer) {
                if let Some(metadata) =
                    get_json::<OAuthServerMetadata>(&client, &metadata_url).await
                {
                    return Some(metadata);
                }
            }
        }
    }
    for metadata_url in authorization_server_metadata_urls(server_url) {
        if let Some(metadata) =
            get_json::<OAuthServerMetadata>(&client, &metadata_url).await
        {
            return Some(metadata);
        }
    }
    None
}

fn oauth_discovery_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

async fn get_json<T: DeserializeOwned>(client: &reqwest::Client, url: &str) -> Option<T> {
    let response = client
        .get(url)
        .header(ACCEPT, "application/json")
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json::<T>().await.ok()
}

fn protected_resource_metadata_urls(server_url: &str) -> Vec<String> {
    well_known_urls(server_url, "oauth-protected-resource")
}

fn authorization_server_metadata_urls(issuer_or_server_url: &str) -> Vec<String> {
    well_known_urls(issuer_or_server_url, "oauth-authorization-server")
}

fn well_known_urls(url: &str, suffix: &str) -> Vec<String> {
    let Some(origin) = origin(url) else {
        return Vec::new();
    };
    let mut urls = vec![format!("{origin}/.well-known/{suffix}")];
    if let Ok(parsed) = reqwest::Url::parse(url) {
        let path = parsed.path().trim_end_matches('/');
        if !path.is_empty() {
            urls.push(format!("{origin}/.well-known/{suffix}{path}"));
        }
    }
    urls.sort();
    urls.dedup();
    urls
}

pub(crate) fn origin(url: &str) -> Option<String> {
    let scheme_end = url.find("://")?;
    let scheme = &url[..scheme_end];
    let rest = &url[scheme_end + 3..];
    let host = rest.split('/').next()?.trim_end_matches('/');
    if host.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{host}"))
}

#[derive(Debug, Deserialize)]
struct OAuthProtectedResourceMetadata {
    #[serde(default)]
    authorization_servers: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct OAuthServerMetadata {
    #[serde(default)]
    authorization_endpoint: Option<String>,
    #[serde(default)]
    token_endpoint: Option<String>,
    #[serde(default)]
    registration_endpoint: Option<String>,
}
