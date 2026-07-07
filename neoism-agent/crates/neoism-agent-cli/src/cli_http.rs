use anyhow::Context;
use serde_json::Value;

pub(crate) async fn get_and_print(
    server: String,
    path: &str,
    dir: Option<&str>,
    redact: bool,
) -> anyhow::Result<()> {
    let server = normalize_server(&server);
    let client = reqwest::Client::new();
    let mut value = response_json(
        request_with_dir(client.get(format!("{server}{path}")), dir)
            .send()
            .await?,
    )
    .await?;
    if redact {
        redact_secrets(&mut value);
    }
    print_json(value)
}

pub(crate) async fn post_and_print(
    server: String,
    path: &str,
    dir: Option<&str>,
    body: Value,
) -> anyhow::Result<()> {
    let server = normalize_server(&server);
    let client = reqwest::Client::new();
    let builder = request_with_dir(client.post(format!("{server}{path}")), dir);
    let builder = if body.is_null() {
        builder
    } else {
        builder.json(&body)
    };
    let value = response_json(builder.send().await?).await?;
    print_json(value)
}

pub(crate) fn request_with_dir(
    builder: reqwest::RequestBuilder,
    dir: Option<&str>,
) -> reqwest::RequestBuilder {
    match dir {
        Some(dir) => builder.query(&[("directory", dir)]),
        None => builder,
    }
}

pub(crate) fn normalize_server(server: &str) -> String {
    server.trim_end_matches('/').to_string()
}

pub(crate) async fn response_json(response: reqwest::Response) -> anyhow::Result<Value> {
    let status = response.status();
    let url = response.url().clone();
    let text = response
        .text()
        .await
        .with_context(|| format!("failed to read response body from {url}"))?;
    if !status.is_success() {
        anyhow::bail!("HTTP {status} for {url}: {text}");
    }
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text)
        .with_context(|| format!("failed to parse JSON response from {url}: {text}"))
}

pub(crate) fn print_json(value: Value) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

pub(crate) fn redact_secrets(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if matches!(
                    key.as_str(),
                    "key"
                        | "access"
                        | "refresh"
                        | "token"
                        | "accessToken"
                        | "refreshToken"
                ) {
                    *value = Value::String("<redacted>".to_string());
                } else {
                    redact_secrets(value);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_secrets(item);
            }
        }
        _ => {}
    }
}
