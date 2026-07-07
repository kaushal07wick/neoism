local M = {}

local trigger_timer = nil
local WORD_TRIGGER_DELAY_MS = 30
local MIN_TRIGGER_INTERVAL_MS = 45
local REQUEST_COOLDOWN_MS = 90
local SUPPRESS_AFTER_ACCEPT_MS = 1200
local SUPPRESS_AFTER_CANCEL_MS = 200
local MIN_WORD_PREFIX = 1
local last_trigger_ms = 0
local request_busy_until_ms = 0
local suppress_until_ms = 0
local uv = vim.uv or vim.loop
local clear_trigger_timer
local completion_prefix_len

local function lsp_log(event, fields)
  if vim.env.NEOISM_LSP_LOG == nil or vim.env.NEOISM_LSP_LOG == "" then
    return
  end
  local ok, events = pcall(require, "rio.events")
  if ok and type(events.lsp_log) == "function" then
    events.lsp_log(event, fields or {})
  end
end

local function client_names(clients)
  local names = {}
  for _, client in ipairs(clients or {}) do
    names[#names + 1] = client.name or tostring(client.id or "?")
  end
  return names
end

local function client_supports_completion(client, buf)
  if client and client.supports_method then
    local ok, supports = pcall(client.supports_method, client, "textDocument/completion", buf)
    if ok then
      return supports
    end
  end
  local caps = (client and client.server_capabilities) or {}
  return caps.completionProvider ~= nil
end

local function char_is_lsp_trigger(ch, clients)
  if ch == "" then
    return false
  end
  for _, client in ipairs(clients or {}) do
    local provider = client.server_capabilities and client.server_capabilities.completionProvider
    local chars = provider and provider.triggerCharacters or nil
    if type(chars) == "table" then
      for _, trigger in ipairs(chars) do
        if trigger == ch then
          return true
        end
      end
    end
  end
  return false
end

local function completion_clients(buf)
  if not vim.lsp then
    return {}
  end
  local active = {}
  if vim.lsp.get_clients then
    active = vim.lsp.get_clients({ bufnr = buf })
  elseif vim.lsp.get_active_clients then
    active = vim.lsp.get_active_clients({ bufnr = buf })
  end

  local clients = {}
  for _, client in ipairs(active or {}) do
    if client_supports_completion(client, buf) then
      table.insert(clients, client)
    end
  end
  return clients
end

local function can_use_completion_get()
  return vim.lsp and vim.lsp.completion and type(vim.lsp.completion.get) == "function"
end

local function prepare_lsp_omnifunc()
  if not vim.lsp or not vim.lsp.omnifunc then
    return false
  end
  return pcall(vim.api.nvim_buf_set_option, 0, "omnifunc", "v:lua.vim.lsp.omnifunc")
end

local function request_completion(clients, reason)
  if can_use_completion_get() then
    local ok, err = pcall(vim.lsp.completion.get)
    lsp_log("completion.request", {
      method = "completion_get",
      ok = ok,
      error = (not ok) and tostring(err) or nil,
      reason = reason or "",
      clients = client_names(clients),
      prefix_len = completion_prefix_len(),
    })
    return ok
  end
  if not prepare_lsp_omnifunc() then
    lsp_log("completion.request", {
      method = "omnifunc",
      ok = false,
      error = "omnifunc unavailable",
      reason = reason or "",
      clients = client_names(clients),
      prefix_len = completion_prefix_len(),
    })
    return false
  end
  local keys = vim.api.nvim_replace_termcodes("<C-x><C-o>", true, false, true)
  vim.api.nvim_feedkeys(keys, "n", false)
  lsp_log("completion.request", {
    method = "omnifunc",
    ok = true,
    reason = reason or "",
    clients = client_names(clients),
    prefix_len = completion_prefix_len(),
  })
  return true
end

function M.suppress_for(ms)
  local now = uv.now()
  local until_ms = now + (ms or SUPPRESS_AFTER_CANCEL_MS)
  suppress_until_ms = math.max(suppress_until_ms, until_ms)
  request_busy_until_ms = math.max(request_busy_until_ms, suppress_until_ms)
  clear_trigger_timer()
end

function M.suppression_remaining_ms()
  local remaining = suppress_until_ms - uv.now()
  if remaining > 0 then
    return remaining
  end
  return 0
end

function M.is_suppressed()
  return vim.fn.pumvisible() == 1 or M.suppression_remaining_ms() > 0
end

local function schedule_trigger_after(delay_ms)
  if trigger_timer then
    trigger_timer:stop()
    trigger_timer:close()
    trigger_timer = nil
  end
  trigger_timer = vim.defer_fn(function()
    trigger_timer = nil
    M.trigger("timer")
  end, math.max(WORD_TRIGGER_DELAY_MS, delay_ms or WORD_TRIGGER_DELAY_MS))
end

function M.trigger(reason)
  if vim.fn.mode() ~= "i" or vim.fn.pumvisible() == 1 then
    lsp_log("completion.trigger", {
      decision = "skip_mode_or_popup",
      reason = reason or "",
      mode = vim.fn.mode(),
      pumvisible = vim.fn.pumvisible(),
    })
    return
  end
  local buf = vim.api.nvim_get_current_buf()
  local clients = completion_clients(buf)
  if #clients == 0 then
    lsp_log("completion.trigger", {
      decision = "skip_no_clients",
      reason = reason or "",
      buf = buf,
      filetype = vim.bo[buf].filetype or "",
      prefix_len = completion_prefix_len(),
    })
    return
  end
  local now = uv.now()
  if M.is_suppressed() then
    lsp_log("completion.trigger", {
      decision = "skip_suppressed",
      reason = reason or "",
      buf = buf,
      filetype = vim.bo[buf].filetype or "",
      suppression_remaining_ms = M.suppression_remaining_ms(),
      clients = client_names(clients),
      prefix_len = completion_prefix_len(),
    })
    return
  end
  if now < request_busy_until_ms then
    local remaining = request_busy_until_ms - now
    lsp_log("completion.trigger", {
      decision = "reschedule_busy",
      reason = reason or "",
      buf = buf,
      filetype = vim.bo[buf].filetype or "",
      cooldown_remaining_ms = remaining,
      clients = client_names(clients),
      prefix_len = completion_prefix_len(),
    })
    schedule_trigger_after(remaining + 8)
    return
  end
  local interval_remaining = MIN_TRIGGER_INTERVAL_MS - (now - last_trigger_ms)
  if interval_remaining > 0 then
    lsp_log("completion.trigger", {
      decision = "reschedule_interval",
      reason = reason or "",
      buf = buf,
      filetype = vim.bo[buf].filetype or "",
      cooldown_remaining_ms = interval_remaining,
      clients = client_names(clients),
      prefix_len = completion_prefix_len(),
    })
    schedule_trigger_after(interval_remaining + 8)
    return
  end
  last_trigger_ms = now
  lsp_log("completion.trigger", {
    decision = "request",
    reason = reason or "",
    buf = buf,
    filetype = vim.bo[buf].filetype or "",
    clients = client_names(clients),
    prefix_len = completion_prefix_len(),
  })
  local ok = request_completion(clients, reason)
  request_busy_until_ms = ok and (now + REQUEST_COOLDOWN_MS) or (now + MIN_TRIGGER_INTERVAL_MS)
end

local function schedule_trigger()
  if M.is_suppressed() then
    clear_trigger_timer()
    return
  end
  schedule_trigger_after(WORD_TRIGGER_DELAY_MS)
end

local function schedule_trigger_after_insert_char()
  local ch = vim.v.char or ""
  local buf = vim.api.nvim_get_current_buf()
  local clients = completion_clients(buf)
  local word_char = ch:match("[%w_]") ~= nil
  local trigger_char = char_is_lsp_trigger(ch, clients)
  if ch == "" or (not word_char and not trigger_char) then
    clear_trigger_timer()
    lsp_log("completion.insert_char", {
      decision = "skip_non_trigger",
      char = ch,
      buf = buf,
      filetype = vim.bo[buf].filetype or "",
      clients = client_names(clients),
    })
    return
  end
  vim.schedule(function()
    if vim.fn.mode() ~= "i" then
      return
    end
    local prefix_len = completion_prefix_len()
    if trigger_char or prefix_len >= MIN_WORD_PREFIX then
      lsp_log("completion.insert_char", {
        decision = "schedule",
        char = ch,
        lsp_trigger = trigger_char,
        buf = buf,
        filetype = vim.bo[buf].filetype or "",
        clients = client_names(clients),
        prefix_len = prefix_len,
      })
      schedule_trigger()
    else
      clear_trigger_timer()
      lsp_log("completion.insert_char", {
        decision = "clear_short_prefix",
        char = ch,
        buf = buf,
        filetype = vim.bo[buf].filetype or "",
        clients = client_names(clients),
        prefix_len = prefix_len,
      })
    end
  end)
end

function clear_trigger_timer()
  if trigger_timer then
    trigger_timer:stop()
    trigger_timer:close()
    trigger_timer = nil
  end
end

local function suppress_after_complete()
  local reason = vim.v.event and vim.v.event.reason or ""
  if reason == "accept" then
    lsp_log("completion.done", { reason = reason, suppress_ms = SUPPRESS_AFTER_ACCEPT_MS })
    M.suppress_for(SUPPRESS_AFTER_ACCEPT_MS)
  else
    lsp_log("completion.done", { reason = reason, suppress_ms = SUPPRESS_AFTER_CANCEL_MS })
    M.suppress_for(SUPPRESS_AFTER_CANCEL_MS)
  end
end

function completion_prefix_len()
  local col = vim.api.nvim_win_get_cursor(0)[2]
  if col == 0 then
    return 0
  end
  local line = vim.api.nvim_get_current_line()
  local before_cursor = line:sub(1, col)
  local prefix = before_cursor:match("[%w_]+$")
  return prefix and #prefix or 0
end

function M.setup()
  pcall(function()
    vim.opt.completeopt = { "menuone", "noinsert", "noselect" }
    vim.opt.pumheight = 12

    for name, sign in pairs({ Error = "E", Warn = "W", Info = "I", Hint = "H" }) do
      pcall(vim.fn.sign_define, "DiagnosticSign" .. name, {
        text = sign,
        texthl = "Diagnostic" .. name,
        numhl = "Diagnostic" .. name,
      })
    end

    vim.diagnostic.config({
      underline = true,
      signs = true,
      virtual_text = false,
      virtual_lines = false,
      -- Do NOT recompute/redraw diagnostics while the user is typing in
      -- insert mode. `true` here marked errors live mid-edit (before
      -- save or returning to normal mode) AND fought `rio/lsp.lua`'s
      -- defer-during-insert system, forcing diagnostics through the
      -- fragile defer→InsertLeave-flush path instead of the plain
      -- `emit_diagnostics` path BufEnter uses (the one that reliably
      -- paints the inline lens). With `false`, nvim caches LSP
      -- diagnostics received during insert and applies them on
      -- InsertLeave, firing `DiagnosticChanged` in NORMAL mode → the
      -- same emit path → the inline lens shows up without needing a
      -- buffer switch to force a repaint.
      update_in_insert = false,
      severity_sort = true,
      float = { border = "rounded", source = "if_many" },
    })

    -- No nvim-side completion trigger. LSP completion is owned by the Rust
    -- engine end to end: the frontend requests textDocument/completion on
    -- insert-mode typing and renders + inserts its own popup (daemon
    -- rust_lsp::completion + desktop LspCompletionState). nvim's built-in
    -- keyword completion (Ctrl-N/Ctrl-P) still works via completeopt above.
  end)
end

return M
