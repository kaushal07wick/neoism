use anyhow::Context;
use serde_json::{json, Value};

use crate::chat_session::next_redo_message_id;
use crate::{
    get_and_print, normalize_server, post_and_print, print_json, response_json,
    AuthCommand, McpCommand, SessionCommand, ToolCommand,
};

pub(super) async fn auth(command: AuthCommand) -> anyhow::Result<()> {
    match command {
        AuthCommand::Methods { server } => {
            get_and_print(server, "/provider/auth", None, false).await
        }
        AuthCommand::Status { provider, server } => {
            get_and_print(server, &format!("/auth/{provider}"), None, true).await
        }
        AuthCommand::SetApi {
            provider,
            key,
            server,
        } => {
            let server = normalize_server(&server);
            let value = response_json(
                reqwest::Client::new()
                    .put(format!("{server}/auth/{provider}"))
                    .json(&json!({ "type": "api", "key": key }))
                    .send()
                    .await?,
            )
            .await?;
            print_json(json!({ "provider": provider, "saved": value }))
        }
        AuthCommand::Remove { provider, server } => {
            let server = normalize_server(&server);
            let value = response_json(
                reqwest::Client::new()
                    .delete(format!("{server}/auth/{provider}"))
                    .send()
                    .await?,
            )
            .await?;
            print_json(json!({ "provider": provider, "removed": value }))
        }
        AuthCommand::LoginCodex {
            server,
            browser,
            no_wait,
        } => {
            let server = normalize_server(&server);
            let client = reqwest::Client::new();
            let method = if browser { 0 } else { 1 };
            let authorization = response_json(
                client
                    .post(format!("{server}/provider/openai/oauth/authorize"))
                    .json(&json!({ "method": method, "inputs": {} }))
                    .send()
                    .await?,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&authorization)?);
            if no_wait {
                return Ok(());
            }
            eprintln!("Waiting for OpenAI OAuth completion. Finish the browser/device flow now.");
            let saved = response_json(
                client
                    .post(format!("{server}/provider/openai/oauth/callback"))
                    .json(&json!({ "method": method }))
                    .send()
                    .await?,
            )
            .await?;
            print_json(json!({ "provider": "openai", "saved": saved }))
        }
    }
}

pub(super) async fn mcp(command: McpCommand) -> anyhow::Result<()> {
    match command {
        McpCommand::Status { server, dir } => {
            get_and_print(server, "/mcp", dir.as_deref(), false).await
        }
        McpCommand::Connect { name, server, dir } => {
            post_and_print(
                server,
                &format!("/mcp/{name}/connect"),
                dir.as_deref(),
                Value::Null,
            )
            .await
        }
        McpCommand::Disconnect { name, server, dir } => {
            post_and_print(
                server,
                &format!("/mcp/{name}/disconnect"),
                dir.as_deref(),
                Value::Null,
            )
            .await
        }
        McpCommand::Tools { name, server, dir } => {
            get_and_print(server, &format!("/mcp/{name}/tools"), dir.as_deref(), false)
                .await
        }
        McpCommand::Resources { name, server, dir } => {
            get_and_print(
                server,
                &format!("/mcp/{name}/resources"),
                dir.as_deref(),
                false,
            )
            .await
        }
        McpCommand::Prompts { name, server, dir } => {
            get_and_print(
                server,
                &format!("/mcp/{name}/prompts"),
                dir.as_deref(),
                false,
            )
            .await
        }
        McpCommand::AuthStart { name, server, dir } => {
            post_and_print(
                server,
                &format!("/mcp/{name}/auth"),
                dir.as_deref(),
                Value::Null,
            )
            .await
        }
        McpCommand::AuthFinish {
            name,
            code,
            server,
            dir,
        } => {
            post_and_print(
                server,
                &format!("/mcp/{name}/auth/callback"),
                dir.as_deref(),
                json!({ "code": code }),
            )
            .await
        }
    }
}

pub(super) async fn session(command: SessionCommand) -> anyhow::Result<()> {
    match command {
        SessionCommand::List { server, dir } => {
            get_and_print(server, "/session", dir.as_deref(), false).await
        }
        SessionCommand::Get { id, server } => {
            get_and_print(server, &format!("/session/{id}"), None, false).await
        }
        SessionCommand::Messages { id, server, limit } => {
            let path = match limit {
                Some(limit) => format!("/session/{id}/message?limit={limit}"),
                None => format!("/session/{id}/message"),
            };
            get_and_print(server, &path, None, false).await
        }
        SessionCommand::Undo { id, server } => {
            session_undo_or_redo(server, &id, false).await
        }
        SessionCommand::Redo { id, server } => {
            session_undo_or_redo(server, &id, true).await
        }
        SessionCommand::Abort { id, server } => {
            post_and_print(server, &format!("/session/{id}/abort"), None, Value::Null)
                .await
        }
    }
}

async fn session_undo_or_redo(
    server: String,
    id: &str,
    redo: bool,
) -> anyhow::Result<()> {
    let server = normalize_server(&server);
    let client = reqwest::Client::new();
    let tree = response_json(
        client
            .get(format!("{server}/session/{id}/undo/tree"))
            .send()
            .await?,
    )
    .await?;
    let body = if redo {
        let Some(revert_id) = tree
            .get("revert")
            .and_then(|revert| {
                revert.get("messageID").or_else(|| revert.get("messageId"))
            })
            .and_then(Value::as_str)
        else {
            anyhow::bail!("nothing to redo");
        };
        if let Some(next_id) = next_redo_message_id(&tree, revert_id) {
            response_json(
                client
                    .post(format!("{server}/session/{id}/revert"))
                    .json(&json!({ "messageID": next_id }))
                    .send()
                    .await?,
            )
            .await?
        } else {
            response_json(
                client
                    .post(format!("{server}/session/{id}/unrevert"))
                    .send()
                    .await?,
            )
            .await?
        }
    } else {
        let Some(message_id) = tree
            .get("cursor")
            .and_then(|cursor| {
                cursor.get("messageID").or_else(|| cursor.get("messageId"))
            })
            .and_then(Value::as_str)
        else {
            anyhow::bail!("nothing to undo");
        };
        response_json(
            client
                .post(format!("{server}/session/{id}/revert"))
                .json(&json!({ "messageID": message_id }))
                .send()
                .await?,
        )
        .await?
    };
    print_json(body)
}

pub(super) async fn tool(command: ToolCommand) -> anyhow::Result<()> {
    match command {
        ToolCommand::List { server, dir } => {
            get_and_print(server, "/experimental/tool", dir.as_deref(), false).await
        }
        ToolCommand::Ids { server, dir } => {
            get_and_print(server, "/experimental/tool/ids", dir.as_deref(), false).await
        }
        ToolCommand::Exec {
            id,
            server,
            dir,
            args,
        } => {
            let arguments: Value = serde_json::from_str(&args)
                .with_context(|| "--args must be a valid JSON object")?;
            post_and_print(
                server,
                &format!("/experimental/tool/{id}/execute"),
                dir.as_deref(),
                arguments,
            )
            .await
        }
    }
}
