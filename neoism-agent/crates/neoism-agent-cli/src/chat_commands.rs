use crate::{BOLD, CYAN, DIM, RESET};

pub(crate) struct ChatCommandSpec {
    pub(crate) names: &'static [&'static str],
    pub(crate) usage: &'static str,
    pub(crate) category: &'static str,
    pub(crate) description: &'static str,
}

pub(crate) const CHAT_COMMANDS: &[ChatCommandSpec] = &[
    ChatCommandSpec {
        names: &["/"],
        usage: "/",
        category: "Core",
        description: "Open the command menu.",
    },
    ChatCommandSpec {
        names: &["/help", "/?"],
        usage: "/help [query]",
        category: "Core",
        description: "Show or search commands.",
    },
    ChatCommandSpec {
        names: &["/quit", "/exit", "/q"],
        usage: "/quit",
        category: "Core",
        description: "Exit chat.",
    },
    ChatCommandSpec {
        names: &["/clear"],
        usage: "/clear",
        category: "Core",
        description: "Clear the terminal.",
    },
    ChatCommandSpec {
        names: &["/model"],
        usage: "/model [provider/model]",
        category: "Model",
        description: "Show or set the active model.",
    },
    ChatCommandSpec {
        names: &["/models"],
        usage: "/models [provider]",
        category: "Model",
        description: "List model ids from the provider catalog.",
    },
    ChatCommandSpec {
        names: &["/think", "/reasoning"],
        usage: "/think [off|none|low|medium|high|xhigh]",
        category: "Model",
        description: "Set the active model variant or reasoning effort.",
    },
    ChatCommandSpec {
        names: &["/agent"],
        usage: "/agent [name]",
        category: "Agent",
        description: "Show or switch the active agent.",
    },
    ChatCommandSpec {
        names: &["/agents"],
        usage: "/agents",
        category: "Agent",
        description: "List configured native and custom agents.",
    },
    ChatCommandSpec {
        names: &["/sub-agent", "/subagents", "/sub"],
        usage: "/sub-agent",
        category: "Agent",
        description: "Open this session's main/subagent session picker.",
    },
    ChatCommandSpec {
        names: &["/session", "/sessions", "/ses"],
        usage: "/sessions",
        category: "Session",
        description: "Open the root session picker.",
    },
    ChatCommandSpec {
        names: &["/messages"],
        usage: "/messages [limit]",
        category: "Session",
        description: "Print a compact transcript.",
    },
    ChatCommandSpec {
        names: &["/compact"],
        usage: "/compact",
        category: "Session",
        description: "Compact the current session context.",
    },
    ChatCommandSpec {
        names: &["/goal"],
        usage: "/goal [text|clear]",
        category: "Session",
        description: "Show, set, or clear the persistent session goal.",
    },
    ChatCommandSpec {
        names: &["/expand", "/open"],
        usage: "/expand",
        category: "Session",
        description: "Expand the newest truncated tool or subagent result.",
    },
    ChatCommandSpec {
        names: &["/undo"],
        usage: "/undo",
        category: "Session",
        description: "Undo the last user turn.",
    },
    ChatCommandSpec {
        names: &["/redo"],
        usage: "/redo",
        category: "Session",
        description: "Redo the last undone turn.",
    },
    ChatCommandSpec {
        names: &["/new"],
        usage: "/new",
        category: "Session",
        description: "Create a new session.",
    },
    ChatCommandSpec {
        names: &["/abort"],
        usage: "/abort",
        category: "Session",
        description: "Abort the active run.",
    },
    ChatCommandSpec {
        names: &["/queue"],
        usage: "/queue [clear|pop]",
        category: "Session",
        description: "Inspect or manage queued turns.",
    },
    ChatCommandSpec {
        names: &["/permissions"],
        usage: "/permissions",
        category: "Session",
        description: "List pending permission requests.",
    },
    ChatCommandSpec {
        names: &["/permit"],
        usage: "/permit [once|always|reject] [id]",
        category: "Session",
        description: "Reply to a pending permission request.",
    },
    ChatCommandSpec {
        names: &["/questions"],
        usage: "/questions",
        category: "Session",
        description: "List pending question requests.",
    },
    ChatCommandSpec {
        names: &["/answer"],
        usage: "/answer <text>",
        category: "Session",
        description: "Answer the first pending question.",
    },
    ChatCommandSpec {
        names: &["/reject", "/deny"],
        usage: "/reject [id]",
        category: "Session",
        description: "Reject an active permission or pending question.",
    },
    ChatCommandSpec {
        names: &["/tools"],
        usage: "/tools",
        category: "Workspace",
        description: "List available built-in, MCP, and plugin tools.",
    },
    ChatCommandSpec {
        names: &["/skills", "/skill"],
        usage: "/skills",
        category: "Workspace",
        description: "List discovered SKILL.md skills for this workspace.",
    },
    ChatCommandSpec {
        names: &["/mcp"],
        usage: "/mcp",
        category: "Workspace",
        description: "Show MCP server status for this workspace.",
    },
    ChatCommandSpec {
        names: &["/doctor"],
        usage: "/doctor",
        category: "Workspace",
        description: "Show server and workspace health.",
    },
    ChatCommandSpec {
        names: &["/providers"],
        usage: "/providers",
        category: "Provider",
        description: "Show configured providers.",
    },
    ChatCommandSpec {
        names: &["/auth"],
        usage: "/auth",
        category: "Provider",
        description: "Show redacted OpenAI auth status.",
    },
];

pub(crate) fn print_chat_command_menu(filter: Option<&str>) {
    let query = filter
        .unwrap_or_default()
        .trim()
        .trim_start_matches('/')
        .to_ascii_lowercase();
    let mut active_category = "";
    let mut count = 0usize;
    println!();
    println!("{BOLD}/ commands{RESET}");
    for spec in CHAT_COMMANDS {
        if !query.is_empty() && !command_matches_query(spec, &query) {
            continue;
        }
        if active_category != spec.category {
            active_category = spec.category;
            println!("{DIM}{active_category}{RESET}");
        }
        println!("  {CYAN}{:<28}{RESET} {}", spec.usage, spec.description);
        count += 1;
    }
    if count == 0 {
        println!("  no commands matched {query}");
    }
    println!();
}

pub(crate) fn command_matches_query(spec: &ChatCommandSpec, query: &str) -> bool {
    spec.usage.to_ascii_lowercase().contains(query)
        || spec.description.to_ascii_lowercase().contains(query)
        || spec
            .names
            .iter()
            .any(|name| name.trim_start_matches('/').contains(query))
}
