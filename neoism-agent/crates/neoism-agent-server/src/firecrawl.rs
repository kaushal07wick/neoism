//! Minimal firecrawl.dev client used to back goal-oriented web research.
//!
//! Firecrawl is a hosted scraping/crawling API. We use its `/v1/scrape`
//! endpoint to fetch a single URL and return its Markdown rendering. The whole
//! feature is gated behind the `FIRECRAWL_API_KEY` environment variable: when
//! it is absent, [`firecrawl_enabled`] returns `false` and callers should skip
//! research instead of erroring.

use std::time::Duration;

use anyhow::{anyhow, Context};
use serde::Deserialize;
use serde_json::json;

/// Environment variable holding the firecrawl API key.
pub(crate) const FIRECRAWL_API_KEY_ENV: &str = "FIRECRAWL_API_KEY";

/// Environment variable allowing the firecrawl base URL to be overridden
/// (e.g. for self-hosted firecrawl). Defaults to the hosted API.
const FIRECRAWL_BASE_URL_ENV: &str = "FIRECRAWL_BASE_URL";

const DEFAULT_BASE_URL: &str = "https://api.firecrawl.dev";

/// Maximum number of characters of scraped content we keep per page so a single
/// research note cannot blow up the session context window.
const MAX_CONTENT_CHARS: usize = 8_000;

/// Result of scraping a single page via firecrawl.
#[derive(Clone, Debug)]
pub(crate) struct FirecrawlPage {
    /// The URL that was scraped.
    pub(crate) url: String,
    /// Page title, when firecrawl reported one.
    pub(crate) title: Option<String>,
    /// Markdown rendering of the page (truncated to [`MAX_CONTENT_CHARS`]).
    pub(crate) markdown: String,
}

/// Returns the configured firecrawl API key, if present and non-empty.
pub(crate) fn firecrawl_api_key() -> Option<String> {
    std::env::var(FIRECRAWL_API_KEY_ENV)
        .ok()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
}

/// Whether firecrawl-backed research is available (i.e. an API key is set).
pub(crate) fn firecrawl_enabled() -> bool {
    firecrawl_api_key().is_some()
}

fn base_url() -> String {
    std::env::var(FIRECRAWL_BASE_URL_ENV)
        .ok()
        .map(|url| url.trim().trim_end_matches('/').to_string())
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

#[derive(Debug, Deserialize)]
struct ScrapeResponse {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    data: Option<ScrapeData>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ScrapeData {
    #[serde(default)]
    markdown: Option<String>,
    #[serde(default)]
    metadata: Option<ScrapeMetadata>,
}

#[derive(Debug, Deserialize)]
struct ScrapeMetadata {
    #[serde(default)]
    title: Option<String>,
}

/// Scrapes a single URL via the firecrawl `/v1/scrape` endpoint.
///
/// Returns an error if firecrawl is not configured, the request fails, or the
/// API responds with a non-success payload. The returned markdown is truncated
/// to a bounded length so it is safe to embed in the model context.
pub(crate) async fn scrape_url(url: &str) -> anyhow::Result<FirecrawlPage> {
    let api_key = firecrawl_api_key().ok_or_else(|| {
        anyhow!("{FIRECRAWL_API_KEY_ENV} is not set; web research is disabled")
    })?;
    let url = url.trim();
    if url.is_empty() {
        return Err(anyhow!("cannot scrape an empty URL"));
    }

    let endpoint = format!("{}/v1/scrape", base_url());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .context("failed to build firecrawl HTTP client")?;

    let response = client
        .post(&endpoint)
        .bearer_auth(&api_key)
        .header("content-type", "application/json")
        .json(&json!({
            "url": url,
            "formats": ["markdown"],
            "onlyMainContent": true,
        }))
        .send()
        .await
        .with_context(|| format!("failed to reach firecrawl at {endpoint}"))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read firecrawl response body")?;
    if !status.is_success() {
        return Err(anyhow!(
            "firecrawl scrape failed with HTTP {status}: {}",
            truncate(&body, 500)
        ));
    }

    let parsed: ScrapeResponse = serde_json::from_str(&body).with_context(|| {
        format!(
            "failed to parse firecrawl response: {}",
            truncate(&body, 500)
        )
    })?;
    if !parsed.success {
        return Err(anyhow!(
            "firecrawl reported failure: {}",
            parsed.error.unwrap_or_else(|| "unknown error".to_string())
        ));
    }

    let data = parsed
        .data
        .ok_or_else(|| anyhow!("firecrawl response missing data field"))?;
    let markdown = data
        .markdown
        .filter(|markdown| !markdown.trim().is_empty())
        .ok_or_else(|| anyhow!("firecrawl returned no markdown content for {url}"))?;

    Ok(FirecrawlPage {
        url: url.to_string(),
        title: data.metadata.and_then(|metadata| metadata.title),
        markdown: truncate(&markdown, MAX_CONTENT_CHARS),
    })
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("\n…(truncated)");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_keeps_short_text() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_clips_long_text() {
        let clipped = truncate(&"a".repeat(20), 5);
        assert!(clipped.starts_with("aaaaa"));
        assert!(clipped.contains("truncated"));
    }
}
