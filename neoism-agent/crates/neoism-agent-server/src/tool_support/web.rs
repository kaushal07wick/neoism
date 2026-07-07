use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use serde_json::{json, Value};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::sleep;

use super::args::{required_string, usize_arg};
use super::{ToolContext, ToolExecutionResult};

const MAX_WEB_BODY_BYTES: usize = 200_000;
const MAX_BATCH_ITEMS: usize = 20;
const DEFAULT_CONCURRENCY: usize = 4;
const MAX_CONCURRENCY: usize = 8;
const DEFAULT_RETRIES: usize = 1;
const MAX_RETRIES: usize = 3;
const DEFAULT_PER_ITEM_LIMIT: usize = 12_000;
const MAX_PER_ITEM_LIMIT: usize = 50_000;

pub(super) async fn webfetch_tool(
    context: ToolContext,
    arguments: Value,
) -> anyhow::Result<ToolExecutionResult> {
    let url = required_string(&arguments, "url")?;
    context.ensure_allowed("webfetch", url)?;
    let page = fetch_url(web_client()?, url.to_string()).await?;

    Ok(ToolExecutionResult {
        title: format!("Fetch {url}"),
        output: page.output,
        metadata: Some(json!({
            "url": page.url,
            "status": page.status,
            "contentType": page.content_type,
            "bytes": page.bytes,
            "truncated": page.truncated,
        })),
    })
}

pub(super) async fn webfetch_batch_tool(
    context: ToolContext,
    arguments: Value,
) -> anyhow::Result<ToolExecutionResult> {
    let urls = string_array_or_single_arg(&arguments, "urls", "url")?;
    for url in &urls {
        context.ensure_allowed("webfetch_batch", url)?;
        parse_web_url(url)?;
    }

    let concurrency = bounded_arg(
        &arguments,
        "concurrency",
        DEFAULT_CONCURRENCY,
        MAX_CONCURRENCY,
    );
    let retries = bounded_arg(&arguments, "retries", DEFAULT_RETRIES, MAX_RETRIES);
    let per_item_limit = bounded_arg(
        &arguments,
        "perItemLimit",
        DEFAULT_PER_ITEM_LIMIT,
        MAX_PER_ITEM_LIMIT,
    );
    let client = web_client()?;
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut tasks = JoinSet::new();

    for (index, url) in urls.iter().cloned().enumerate() {
        let client = client.clone();
        let semaphore = Arc::clone(&semaphore);
        tasks.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .map_err(|error| error.to_string())?;
            let (result, attempts) = retry_fetch(client, url.clone(), retries).await;
            Ok::<_, String>(FetchBatchItem {
                index,
                url,
                attempts,
                result: result.map_err(|error| error.to_string()),
            })
        });
    }

    let mut items = std::iter::repeat_with(|| None)
        .take(urls.len())
        .collect::<Vec<_>>();
    while let Some(joined) = tasks.join_next().await {
        let item = joined
            .map_err(|error| anyhow::anyhow!("webfetch_batch task failed: {error}"))?
            .map_err(|error| anyhow::anyhow!("webfetch_batch task failed: {error}"))?;
        let index = item.index;
        items[index] = Some(item);
    }
    let items = items
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| anyhow::anyhow!("webfetch_batch lost a result"))?;

    Ok(format_fetch_batch(
        items,
        per_item_limit,
        concurrency,
        retries,
    ))
}

pub(super) async fn websearch_tool(
    context: ToolContext,
    arguments: Value,
) -> anyhow::Result<ToolExecutionResult> {
    let query = required_string(&arguments, "query")?;
    context.ensure_allowed("websearch", query)?;
    let search =
        search_query(web_client()?, websearch_endpoint(), query.to_string()).await?;
    Ok(ToolExecutionResult {
        title: format!("Search {query}"),
        output: search.output,
        metadata: Some(json!({
            "query": search.query,
            "endpoint": search.endpoint,
            "bytes": search.bytes,
            "truncated": search.truncated,
        })),
    })
}

pub(super) async fn websearch_batch_tool(
    context: ToolContext,
    arguments: Value,
) -> anyhow::Result<ToolExecutionResult> {
    let queries = string_array_or_single_arg(&arguments, "queries", "query")?;
    for query in &queries {
        context.ensure_allowed("websearch_batch", query)?;
    }

    let concurrency = bounded_arg(
        &arguments,
        "concurrency",
        DEFAULT_CONCURRENCY,
        MAX_CONCURRENCY,
    );
    let retries = bounded_arg(&arguments, "retries", DEFAULT_RETRIES, MAX_RETRIES);
    let per_item_limit = bounded_arg(
        &arguments,
        "perItemLimit",
        DEFAULT_PER_ITEM_LIMIT,
        MAX_PER_ITEM_LIMIT,
    );
    let client = web_client()?;
    let endpoint = websearch_endpoint();
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut tasks = JoinSet::new();

    for (index, query) in queries.iter().cloned().enumerate() {
        let client = client.clone();
        let endpoint = endpoint.clone();
        let semaphore = Arc::clone(&semaphore);
        tasks.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .map_err(|error| error.to_string())?;
            let (result, attempts) =
                retry_search(client, endpoint, query.clone(), retries).await;
            Ok::<_, String>(SearchBatchItem {
                index,
                query,
                attempts,
                result: result.map_err(|error| error.to_string()),
            })
        });
    }

    let mut items = std::iter::repeat_with(|| None)
        .take(queries.len())
        .collect::<Vec<_>>();
    while let Some(joined) = tasks.join_next().await {
        let item = joined
            .map_err(|error| anyhow::anyhow!("websearch_batch task failed: {error}"))?
            .map_err(|error| anyhow::anyhow!("websearch_batch task failed: {error}"))?;
        let index = item.index;
        items[index] = Some(item);
    }
    let items = items
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| anyhow::anyhow!("websearch_batch lost a result"))?;

    Ok(format_search_batch(
        items,
        per_item_limit,
        concurrency,
        retries,
    ))
}

#[derive(Debug)]
struct WebFetchResult {
    url: String,
    status: u16,
    content_type: Option<String>,
    bytes: usize,
    truncated: bool,
    output: String,
}

#[derive(Debug)]
struct WebSearchResult {
    query: String,
    endpoint: String,
    bytes: usize,
    truncated: bool,
    output: String,
}

#[derive(Debug)]
struct FetchBatchItem {
    index: usize,
    url: String,
    attempts: usize,
    result: Result<WebFetchResult, String>,
}

#[derive(Debug)]
struct SearchBatchItem {
    index: usize,
    query: String,
    attempts: usize,
    result: Result<WebSearchResult, String>,
}

fn web_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .with_context(|| "failed to build web client")
}

fn websearch_endpoint() -> String {
    std::env::var("NEOISM_AGENT_WEBSEARCH_ENDPOINT")
        .unwrap_or_else(|_| "https://duckduckgo.com/html/".to_string())
}

async fn fetch_url(
    client: reqwest::Client,
    url: String,
) -> anyhow::Result<WebFetchResult> {
    let parsed = parse_web_url(&url)?;
    let response = client
        .get(parsed.clone())
        .header(
            "user-agent",
            format!("neoism-agent/{}", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .with_context(|| format!("failed to fetch {url}"))?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("failed to read response body from {url}"))?;
    let (output, truncated) = render_web_body(&bytes);
    Ok(WebFetchResult {
        url: parsed.to_string(),
        status: status.as_u16(),
        content_type,
        bytes: bytes.len(),
        truncated,
        output,
    })
}

async fn search_query(
    client: reqwest::Client,
    endpoint: String,
    query: String,
) -> anyhow::Result<WebSearchResult> {
    let response = client
        .get(&endpoint)
        .query(&[("q", query.as_str())])
        .header(
            "user-agent",
            format!("neoism-agent/{}", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .with_context(|| format!("failed to search web for {query}"))?;
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("web search provider returned {status}");
    }
    let bytes = response
        .bytes()
        .await
        .with_context(|| "failed to read web search response")?;
    let (output, truncated) = render_web_body(&bytes);
    Ok(WebSearchResult {
        query,
        endpoint,
        bytes: bytes.len(),
        truncated,
        output,
    })
}

async fn retry_fetch(
    client: reqwest::Client,
    url: String,
    retries: usize,
) -> (anyhow::Result<WebFetchResult>, usize) {
    let mut last_error = None;
    for attempt in 0..=retries {
        match fetch_url(client.clone(), url.clone()).await {
            Ok(result) => return (Ok(result), attempt + 1),
            Err(error) => {
                last_error = Some(error);
                if attempt < retries {
                    sleep(backoff(attempt)).await;
                }
            }
        }
    }
    (
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("fetch failed"))),
        retries + 1,
    )
}

async fn retry_search(
    client: reqwest::Client,
    endpoint: String,
    query: String,
    retries: usize,
) -> (anyhow::Result<WebSearchResult>, usize) {
    let mut last_error = None;
    for attempt in 0..=retries {
        match search_query(client.clone(), endpoint.clone(), query.clone()).await {
            Ok(result) => return (Ok(result), attempt + 1),
            Err(error) => {
                last_error = Some(error);
                if attempt < retries {
                    sleep(backoff(attempt)).await;
                }
            }
        }
    }
    (
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("search failed"))),
        retries + 1,
    )
}

fn backoff(attempt: usize) -> Duration {
    Duration::from_millis(150 * (attempt as u64 + 1))
}

fn parse_web_url(url: &str) -> anyhow::Result<reqwest::Url> {
    let parsed =
        reqwest::Url::parse(url).with_context(|| format!("invalid URL {url}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        anyhow::bail!("web tools only support http and https URLs");
    }
    Ok(parsed)
}

pub(super) fn string_array_or_single_arg(
    arguments: &Value,
    array_key: &str,
    single_key: &str,
) -> anyhow::Result<Vec<String>> {
    let values = if let Some(array) = arguments.get(array_key).and_then(Value::as_array) {
        array
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
                    .ok_or_else(|| anyhow::anyhow!("{array_key} entries must be strings"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?
    } else if let Some(value) = arguments
        .get(single_key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        vec![value.to_string()]
    } else {
        anyhow::bail!("tool argument {array_key} is required");
    };

    if values.is_empty() {
        anyhow::bail!("tool argument {array_key} must not be empty");
    }
    if values.len() > MAX_BATCH_ITEMS {
        anyhow::bail!("{array_key} is limited to {MAX_BATCH_ITEMS} entries");
    }
    Ok(values)
}

pub(super) fn bounded_arg(
    arguments: &Value,
    key: &str,
    default: usize,
    max: usize,
) -> usize {
    usize_arg(arguments, key).unwrap_or(default).clamp(1, max)
}

fn format_fetch_batch(
    items: Vec<FetchBatchItem>,
    per_item_limit: usize,
    concurrency: usize,
    retries: usize,
) -> ToolExecutionResult {
    let ok = items.iter().filter(|item| item.result.is_ok()).count();
    let failed = items.len() - ok;
    let mut output = String::new();
    let mut metadata_items = Vec::new();

    for item in &items {
        match &item.result {
            Ok(page) => {
                let (body, output_truncated) = limit_text(&page.output, per_item_limit);
                output.push_str(&format!(
                    "## {}. {}\nStatus: {} | bytes: {} | attempts: {}\n\n{}\n\n",
                    item.index + 1,
                    page.url,
                    page.status,
                    page.bytes,
                    item.attempts,
                    body
                ));
                metadata_items.push(json!({
                    "url": page.url,
                    "status": page.status,
                    "contentType": page.content_type,
                    "bytes": page.bytes,
                    "truncated": page.truncated,
                    "outputTruncated": output_truncated,
                    "attempts": item.attempts,
                    "ok": true,
                }));
            }
            Err(error) => {
                output.push_str(&format!(
                    "## {}. {}\nError after {} attempt(s): {}\n\n",
                    item.index + 1,
                    item.url,
                    item.attempts,
                    error
                ));
                metadata_items.push(json!({
                    "url": item.url,
                    "error": error,
                    "attempts": item.attempts,
                    "ok": false,
                }));
            }
        }
    }

    ToolExecutionResult {
        title: format!("Fetched {} URLs ({ok} ok, {failed} failed)", items.len()),
        output: output.trim_end().to_string(),
        metadata: Some(json!({
            "count": items.len(),
            "ok": ok,
            "failed": failed,
            "concurrency": concurrency,
            "retries": retries,
            "items": metadata_items,
        })),
    }
}

fn format_search_batch(
    items: Vec<SearchBatchItem>,
    per_item_limit: usize,
    concurrency: usize,
    retries: usize,
) -> ToolExecutionResult {
    let ok = items.iter().filter(|item| item.result.is_ok()).count();
    let failed = items.len() - ok;
    let mut output = String::new();
    let mut metadata_items = Vec::new();

    for item in &items {
        match &item.result {
            Ok(search) => {
                let (body, output_truncated) = limit_text(&search.output, per_item_limit);
                output.push_str(&format!(
                    "## {}. {}\nEndpoint: {} | bytes: {} | attempts: {}\n\n{}\n\n",
                    item.index + 1,
                    search.query,
                    search.endpoint,
                    search.bytes,
                    item.attempts,
                    body
                ));
                metadata_items.push(json!({
                    "query": search.query,
                    "endpoint": search.endpoint,
                    "bytes": search.bytes,
                    "truncated": search.truncated,
                    "outputTruncated": output_truncated,
                    "attempts": item.attempts,
                    "ok": true,
                }));
            }
            Err(error) => {
                output.push_str(&format!(
                    "## {}. {}\nError after {} attempt(s): {}\n\n",
                    item.index + 1,
                    item.query,
                    item.attempts,
                    error
                ));
                metadata_items.push(json!({
                    "query": item.query,
                    "error": error,
                    "attempts": item.attempts,
                    "ok": false,
                }));
            }
        }
    }

    ToolExecutionResult {
        title: format!(
            "Searched {} queries ({ok} ok, {failed} failed)",
            items.len()
        ),
        output: output.trim_end().to_string(),
        metadata: Some(json!({
            "count": items.len(),
            "ok": ok,
            "failed": failed,
            "concurrency": concurrency,
            "retries": retries,
            "items": metadata_items,
        })),
    }
}

pub(super) fn limit_text(text: &str, limit: usize) -> (String, bool) {
    if text.len() <= limit {
        return (text.to_string(), false);
    }
    let end = text
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= limit)
        .last()
        .unwrap_or(0);
    let mut limited = text[..end].trim_end().to_string();
    limited.push_str("\n\n(Item output truncated.)");
    (limited, true)
}

pub(super) fn render_web_body(bytes: &[u8]) -> (String, bool) {
    let truncated = bytes.len() > MAX_WEB_BODY_BYTES;
    let bytes = &bytes[..bytes.len().min(MAX_WEB_BODY_BYTES)];
    let text = String::from_utf8_lossy(bytes);
    let mut rendered = String::new();
    let mut in_tag = false;
    let mut last_space = false;
    for ch in text.chars() {
        match ch {
            '<' => {
                in_tag = true;
                if !last_space && !rendered.is_empty() {
                    rendered.push(' ');
                    last_space = true;
                }
            }
            '>' => in_tag = false,
            _ if in_tag => {}
            _ if ch.is_whitespace() => {
                if !last_space && !rendered.is_empty() {
                    rendered.push(' ');
                    last_space = true;
                }
            }
            _ => {
                rendered.push(ch);
                last_space = false;
            }
        }
    }
    if truncated {
        rendered.push_str("\n\n(Output truncated at 200 KB.)");
    }
    (rendered.trim().to_string(), truncated)
}
