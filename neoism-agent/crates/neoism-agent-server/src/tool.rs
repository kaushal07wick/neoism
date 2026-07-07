use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use neoism_agent_core::{PermissionAction, PermissionRule, ToolListItem};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::state::AppState;

#[path = "tool_support/args.rs"]
mod args;
#[path = "tool_support/artifact.rs"]
pub(crate) mod artifact;
#[path = "tool_support/bash.rs"]
mod bash;
#[path = "tool_support/diagnostics.rs"]
mod diagnostics;
#[path = "tool_support/edit_match.rs"]
mod edit_match;
#[path = "tool_support/fff.rs"]
mod fff;
#[path = "tool_support/file.rs"]
mod file;
#[path = "tool_support/format.rs"]
mod format;
#[path = "tool_support/locks.rs"]
mod locks;
#[path = "tool_support/notes.rs"]
mod notes;
#[path = "tool_support/patch.rs"]
mod patch;
#[path = "tool_support/patch_tool.rs"]
mod patch_tool;
#[path = "tool_support/paths.rs"]
mod paths;
#[path = "tool_support/process.rs"]
pub(crate) mod process;
#[path = "tool_registry.rs"]
mod registry;
#[path = "tool_support/search.rs"]
mod search;
#[path = "tool_support/shell_scan.rs"]
pub(crate) mod shell_scan;
#[path = "tool_support/truncate.rs"]
pub(crate) mod truncate;
#[path = "tool_support/web.rs"]
mod web;

use web::{webfetch_batch_tool, webfetch_tool, websearch_batch_tool, websearch_tool};

type ToolFuture<'a> =
    Pin<Box<dyn Future<Output = anyhow::Result<ToolExecutionResult>> + Send + 'a>>;

pub(crate) trait ToolDef: Send + Sync {
    fn item(&self) -> ToolListItem;
    fn execute<'a>(&'a self, context: ToolContext, arguments: Value) -> ToolFuture<'a>;
}

#[derive(Clone)]
pub(crate) struct ToolContext {
    pub(crate) cwd: PathBuf,
    permission_rules: Vec<PermissionRule>,
    env: BTreeMap<String, String>,
    cancel: Option<Arc<AtomicBool>>,
    formatter: Option<Value>,
    state: Option<AppState>,
    session_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolExecutionResult {
    pub(crate) title: String,
    pub(crate) output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) metadata: Option<Value>,
}

#[derive(Clone)]
struct BuiltinTool {
    id: &'static str,
    description: &'static str,
    parameters: Value,
    executor: ToolExecutor,
}

#[derive(Clone, Copy)]
enum ToolExecutor {
    Bash,
    Read,
    ReadMany,
    ReadAround,
    Write,
    Edit,
    List,
    Grep,
    Glob,
    FfFind,
    FfGrep,
    FffMultiGrep,
    ApplyPatch,
    ArtifactRead,
    ArtifactSearch,
    WebFetch,
    WebFetchBatch,
    WebSearch,
    WebSearchBatch,
    Notes,
    Skill,
    Lsp,
    Unsupported,
}

impl ToolContext {
    pub(crate) fn new(cwd: impl Into<PathBuf>) -> Self {
        Self {
            cwd: cwd.into(),
            permission_rules: Vec::new(),
            env: BTreeMap::new(),
            cancel: None,
            formatter: None,
            state: None,
            session_id: None,
        }
    }

    pub(crate) fn with_permissions(
        mut self,
        permissions: BTreeMap<String, Value>,
    ) -> Self {
        self.permission_rules = crate::permission::from_config_map(&permissions);
        self
    }

    pub(crate) fn with_permission_rules(
        mut self,
        permission_rules: Vec<PermissionRule>,
    ) -> Self {
        self.permission_rules = permission_rules;
        self
    }

    pub(crate) fn with_env(mut self, env: BTreeMap<String, String>) -> Self {
        self.env = env;
        self
    }

    pub(crate) fn with_cancel(mut self, cancel: Option<Arc<AtomicBool>>) -> Self {
        self.cancel = cancel;
        self
    }

    pub(crate) fn with_formatter(mut self, formatter: Option<Value>) -> Self {
        self.formatter = formatter;
        self
    }

    pub(crate) fn with_state(mut self, state: Option<AppState>) -> Self {
        self.state = state;
        self
    }

    pub(crate) fn with_session_id(mut self, session_id: Option<String>) -> Self {
        self.session_id = session_id;
        self
    }

    pub(crate) fn state(&self) -> Option<&AppState> {
        self.state.as_ref()
    }

    pub(crate) fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub(crate) fn formatter(&self) -> Option<&Value> {
        self.formatter.as_ref()
    }

    pub(crate) fn ensure_allowed(
        &self,
        permission: &str,
        target: &str,
    ) -> anyhow::Result<()> {
        match self.permission_decision(permission, target) {
            PermissionDecision::Allow => Ok(()),
            PermissionDecision::Ask => {
                anyhow::bail!(
                    "tool permission {permission} for {target} requires approval"
                )
            }
            PermissionDecision::Deny => {
                anyhow::bail!("tool permission {permission} for {target} is denied")
            }
        }
    }

    pub(crate) fn ensure_explicit_allowed(
        &self,
        permission: &str,
        target: &str,
    ) -> anyhow::Result<()> {
        match self.permission_decision(permission, target) {
            PermissionDecision::Allow => Ok(()),
            PermissionDecision::Ask => {
                anyhow::bail!(
                    "tool permission {permission} for {target} requires approval"
                )
            }
            PermissionDecision::Deny => {
                anyhow::bail!("tool permission {permission} for {target} is denied")
            }
        }
    }

    fn permission_decision(&self, permission: &str, target: &str) -> PermissionDecision {
        match crate::permission::evaluate(permission, target, &self.permission_rules)
            .action
        {
            PermissionAction::Allow => PermissionDecision::Allow,
            PermissionAction::Ask => PermissionDecision::Ask,
            PermissionAction::Deny => PermissionDecision::Deny,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PermissionDecision {
    Allow,
    Ask,
    Deny,
}

impl ToolDef for BuiltinTool {
    fn item(&self) -> ToolListItem {
        ToolListItem {
            id: self.id.to_string(),
            description: self.description.to_string(),
            parameters: self.parameters.clone(),
        }
    }

    fn execute<'a>(&'a self, context: ToolContext, arguments: Value) -> ToolFuture<'a> {
        Box::pin(async move {
            match self.executor {
                ToolExecutor::Bash => bash::bash_tool(context, arguments).await,
                ToolExecutor::Read => file::read_tool(context, arguments),
                ToolExecutor::ReadMany => file::read_many_tool(context, arguments),
                ToolExecutor::ReadAround => file::read_around_tool(context, arguments),
                ToolExecutor::Write => file::write_tool(context, arguments).await,
                ToolExecutor::Edit => file::edit_tool(context, arguments).await,
                ToolExecutor::List => file::list_tool(context, arguments),
                ToolExecutor::Grep => file::grep_tool(context, arguments),
                ToolExecutor::Glob => file::glob_tool(context, arguments),
                ToolExecutor::FfFind => fff::fffind_tool(context, arguments).await,
                ToolExecutor::FfGrep => fff::ffgrep_tool(context, arguments).await,
                ToolExecutor::FffMultiGrep => fff::fff_multi_grep_tool(context, arguments).await,
                ToolExecutor::ApplyPatch => patch_tool::apply_patch_tool(context, arguments).await,
                ToolExecutor::ArtifactRead => artifact::read_tool(context, arguments).await,
                ToolExecutor::ArtifactSearch => artifact::search_tool(context, arguments).await,
                ToolExecutor::WebFetch => webfetch_tool(context, arguments).await,
                ToolExecutor::WebFetchBatch => webfetch_batch_tool(context, arguments).await,
                ToolExecutor::WebSearch => websearch_tool(context, arguments).await,
                ToolExecutor::WebSearchBatch => websearch_batch_tool(context, arguments).await,
                ToolExecutor::Notes => notes::notes_tool(context, arguments),
                ToolExecutor::Skill => crate::skill::skill_tool(context, arguments).await,
                ToolExecutor::Lsp => crate::lsp::lsp_tool(context, arguments).await,
                ToolExecutor::Unsupported => anyhow::bail!(
                    "tool {} is registered but execution is waiting for permission/runtime wiring",
                    self.id
                ),
            }
        })
    }
}

pub(crate) fn ids() -> Vec<String> {
    registry::definitions()
        .into_iter()
        .map(|tool| tool.id.to_string())
        .collect()
}

pub(crate) fn list() -> Vec<ToolListItem> {
    registry::definitions()
        .into_iter()
        .map(|tool| tool.item())
        .collect()
}

pub(crate) async fn execute(
    id: &str,
    context: ToolContext,
    arguments: Value,
) -> anyhow::Result<ToolExecutionResult> {
    let id = if id == "patch" { "apply_patch" } else { id };
    let tool = registry::definitions()
        .into_iter()
        .find(|tool| tool.id == id)
        .ok_or_else(|| anyhow::anyhow!("unknown tool {id}"))?;
    tool.execute(context, arguments).await
}

#[cfg(test)]
#[path = "tool_tests.rs"]
mod tests;
