//! Terminal input model the chrome panels render against.
//!
//! `InputBuffer` is the surface `command_composer` and `completion_menu`
//! consume — they only read the shape of the user's pending command,
//! the cursor, completion flash, and the shell kind. Both native (via
//! `frontends/neoism::terminal::blocks::TerminalInputBuffer`) and web
//! (via a wire-message snapshot) implement this trait.
//!
//! The flash and shell-kind types are POD lifted out of
//! `frontends/neoism::terminal::blocks` so the shared panels don't have
//! to depend on native-only state machinery.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalShellKind {
    Bash,
    Zsh,
    Fish,
    Unknown,
}

impl TerminalShellKind {
    pub fn label(self) -> &'static str {
        match self {
            TerminalShellKind::Bash => "bash",
            TerminalShellKind::Zsh => "zsh",
            TerminalShellKind::Fish => "fish",
            TerminalShellKind::Unknown => "sh",
        }
    }

    /// Detect the shell kind from the program path the host launched
    /// (usually `config.shell.program`). Matches by file_name so
    /// `/usr/bin/zsh`, `zsh`, and the login-shell `-zsh` form all
    /// produce `Zsh`.
    pub fn detect(program: &str) -> Self {
        match std::path::Path::new(program)
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.trim_start_matches('-'))
        {
            Some("bash") => TerminalShellKind::Bash,
            Some("zsh") => TerminalShellKind::Zsh,
            Some("fish") => TerminalShellKind::Fish,
            _ => TerminalShellKind::Unknown,
        }
    }

    /// Sanitise + frame the command bytes the host should write into
    /// the PTY when the user submits. Fish wants ` \x08\n` to drop the
    /// inline autosuggestion before executing; every other shell just
    /// needs the newline.
    pub fn command_payload(self, command: &str, _bracketed_paste: bool) -> Vec<u8> {
        let sanitized = crate::terminal_blocks::shell::sanitize_input_text(command);
        let mut bytes = Vec::with_capacity(sanitized.len() + 4);
        bytes.extend_from_slice(sanitized.as_bytes());
        bytes.extend_from_slice(match self {
            TerminalShellKind::Fish => b" \x08\n",
            _ => b"\n",
        });
        bytes
    }
}

/// Live animation parameters the composer needs each frame. Computed
/// upstream from a `CompletionFlash` + elapsed time. `None` once the
/// flash has expired.
#[derive(Debug, Clone, Copy)]
pub enum CompletionFlashState {
    /// `intensity` ramps 1.0 → 0.0 over the success window.
    Success {
        range: (usize, usize),
        intensity: f32,
    },
    /// `shake_offset_logical` is the horizontal offset (in logical
    /// pixels) to apply to the editable run; `intensity` ramps 1.0 →
    /// 0.0 so red tint and shake fade together.
    NoMatch {
        shake_offset_logical: f32,
        intensity: f32,
    },
}

/// Read-only view onto the terminal's pending input the composer
/// renders. Native impl forwards to `TerminalInputBuffer`; web impl
/// reads a per-frame snapshot pushed from the daemon.
pub trait InputBuffer {
    fn text(&self) -> &str;
    fn cursor_byte(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn completion_items(&self) -> &[String];
    fn completion_detail(&self) -> Option<&str> {
        None
    }
    fn flash_state(&self) -> Option<CompletionFlashState>;
    fn control_notice(&self) -> Option<&'static str> {
        None
    }
    fn prompt_burst_elapsed_ms(&self) -> Option<f32>;
    fn suggestion_after_cursor(&self) -> Option<&str>;
    fn is_prompt_animating(&self) -> bool;
    fn shell_kind(&self) -> TerminalShellKind {
        TerminalShellKind::Unknown
    }
}

/// Inert input buffer — empty text, cursor at 0, no completions or
/// suggestions. Useful for the web's first-paint pass before the
/// daemon has pushed real terminal state, and for the chrome assembly
/// to render the composer chassis without an attached terminal.
pub struct NullInputBuffer;

impl InputBuffer for NullInputBuffer {
    fn text(&self) -> &str {
        ""
    }
    fn cursor_byte(&self) -> usize {
        0
    }
    fn is_empty(&self) -> bool {
        true
    }
    fn completion_items(&self) -> &[String] {
        &[]
    }
    fn completion_detail(&self) -> Option<&str> {
        None
    }
    fn flash_state(&self) -> Option<CompletionFlashState> {
        None
    }
    fn control_notice(&self) -> Option<&'static str> {
        None
    }
    fn prompt_burst_elapsed_ms(&self) -> Option<f32> {
        None
    }
    fn suggestion_after_cursor(&self) -> Option<&str> {
        None
    }
    fn is_prompt_animating(&self) -> bool {
        false
    }
}

/// Minimal host-fed input snapshot for frontends that do not yet run
/// the native `TerminalInputBuffer` state machine. It is intentionally
/// read-only from the composer point of view: the host owns mutation
/// and pushes a fresh string/cursor after translating platform input.
#[derive(Debug, Clone)]
pub struct SimpleInputBuffer {
    text: String,
    cursor_byte: usize,
    completion_items: Vec<String>,
    shell_kind: TerminalShellKind,
}

impl Default for SimpleInputBuffer {
    fn default() -> Self {
        Self {
            text: String::new(),
            cursor_byte: 0,
            completion_items: Vec::new(),
            shell_kind: TerminalShellKind::Bash,
        }
    }
}

impl SimpleInputBuffer {
    pub fn set_text(&mut self, text: String) {
        self.cursor_byte = text.len();
        self.text = text;
        self.completion_items.clear();
    }

    pub fn set_snapshot(
        &mut self,
        text: String,
        cursor_byte: usize,
        completion_items: Vec<String>,
    ) {
        self.cursor_byte = cursor_byte.min(text.len());
        self.text = text;
        self.completion_items = completion_items;
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor_byte = 0;
        self.completion_items.clear();
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_shell_kind(&mut self, shell_kind: TerminalShellKind) {
        self.shell_kind = shell_kind;
    }
}

impl InputBuffer for SimpleInputBuffer {
    fn text(&self) -> &str {
        &self.text
    }

    fn cursor_byte(&self) -> usize {
        self.cursor_byte.min(self.text.len())
    }

    fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    fn completion_items(&self) -> &[String] {
        &self.completion_items
    }

    fn flash_state(&self) -> Option<CompletionFlashState> {
        None
    }

    fn prompt_burst_elapsed_ms(&self) -> Option<f32> {
        None
    }

    fn suggestion_after_cursor(&self) -> Option<&str> {
        None
    }

    fn is_prompt_animating(&self) -> bool {
        false
    }

    fn shell_kind(&self) -> TerminalShellKind {
        self.shell_kind
    }
}
