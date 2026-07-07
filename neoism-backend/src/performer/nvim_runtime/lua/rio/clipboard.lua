local M = {}

local function notify(message, level)
  local ok, events = pcall(require, "rio.events")
  if ok and events.notify then
    events.notify(message, level)
  else
    pcall(vim.rpcnotify, 1, "rio_notify", tostring(message), level or "info")
  end
end

local function line_count_message(prefix, count)
  local unit = count == 1 and "line" or "lines"
  return string.format("%s %d %s", prefix, count, unit)
end

function M.lines_to_text(lines, regtype)
  if type(lines) ~= "table" or #lines == 0 then
    return ""
  end

  local text = table.concat(lines, "\n")
  if regtype == "V" then
    text = text .. "\n"
  end
  return text
end

local function register_type_for_text(text, fallback)
  if fallback and fallback ~= "" then
    return fallback
  end
  if type(text) == "string" and text:sub(-1) == "\n" then
    return "V"
  end
  return "v"
end

local function has_clipboard_provider()
  return vim.fn.has("clipboard") == 1
end

local function system_output(cmd)
  local ok, out = pcall(vim.fn.system, cmd)
  if ok and vim.v.shell_error == 0 and type(out) == "string" then
    return out
  end
  return nil
end

local function system_input(cmd, text)
  local ok = pcall(vim.fn.system, cmd, text)
  return ok and vim.v.shell_error == 0
end

local function get_external_clipboard()
  local commands = {
    { exe = "wl-paste", cmd = { "wl-paste", "--type", "text/plain" } },
    { exe = "xclip", cmd = { "xclip", "-selection", "clipboard", "-out" } },
    { exe = "xsel", cmd = { "xsel", "--clipboard", "--output" } },
    { exe = "pbpaste", cmd = { "pbpaste" } },
  }

  for _, entry in ipairs(commands) do
    if vim.fn.executable(entry.exe) == 1 then
      local text = system_output(entry.cmd)
      if text ~= nil then
        return text
      end
    end
  end

  return nil
end

local function set_external_clipboard(text)
  local commands = {
    { exe = "wl-copy", cmd = { "wl-copy" } },
    { exe = "xclip", cmd = { "xclip", "-selection", "clipboard" } },
    { exe = "xsel", cmd = { "xsel", "--clipboard", "--input" } },
    { exe = "pbcopy", cmd = { "pbcopy" } },
  }

  for _, entry in ipairs(commands) do
    if vim.fn.executable(entry.exe) == 1 and system_input(entry.cmd, text) then
      return true
    end
  end

  return false
end

local function set_registers(text, regtype)
  if text == nil or text == "" then
    return false
  end

  regtype = register_type_for_text(text, regtype)
  pcall(vim.fn.setreg, '"', text, regtype)
  if has_clipboard_provider() then
    pcall(vim.fn.setreg, "+", text, regtype)
    pcall(vim.fn.setreg, "*", text, regtype)
  end
  set_external_clipboard(text)
  return true
end

local function split_lines_for_put(text, regtype)
  local lines = vim.split(text, "\n", { plain = true })
  local kind = "c"
  if regtype == "V" or (type(text) == "string" and text:sub(-1) == "\n") then
    kind = "l"
    if lines[#lines] == "" then
      table.remove(lines, #lines)
    end
  end
  if #lines == 0 then
    lines = { "" }
  end
  return lines, kind
end

local function paste_text(text, regtype, after)
  if text == nil or text == "" then
    notify("Clipboard empty", "warn")
    return false
  end

  regtype = register_type_for_text(text, regtype)
  set_registers(text, regtype)

  local mode = vim.fn.mode()
  if mode:match("^[iR]") then
    pcall(vim.api.nvim_paste, text, false, -1)
    return true
  end

  local lines, kind = split_lines_for_put(text, regtype)
  pcall(vim.api.nvim_put, lines, kind, after, true)
  return true
end

local function paste_source()
  local selected = vim.v.register or '"'
  if selected ~= '"' then
    local ok, text = pcall(vim.fn.getreg, selected)
    if ok and text ~= nil and text ~= "" then
      local regtype = ""
      pcall(function()
        regtype = vim.fn.getregtype(selected)
      end)
      return text, regtype
    end
    return nil, nil
  end

  local external = get_external_clipboard()
  if external ~= nil and external ~= "" then
    return external, register_type_for_text(external)
  end

  if has_clipboard_provider() then
    for _, reg in ipairs({ "+", "*" }) do
      local ok, text = pcall(vim.fn.getreg, reg)
      if ok and text ~= nil and text ~= "" then
        local regtype = ""
        pcall(function()
          regtype = vim.fn.getregtype(reg)
        end)
        return text, regtype
      end
    end
  end

  local ok, text = pcall(vim.fn.getreg, '"')
  if ok and text ~= nil and text ~= "" then
    local regtype = ""
    pcall(function()
      regtype = vim.fn.getregtype('"')
    end)
    return text, regtype
  end

  return nil, nil
end

function M.store(text, message)
  if text == nil or text == "" then
    notify("Nothing to copy", "warn")
    return false
  end

  pcall(vim.rpcnotify, 1, "rio_clipboard_store", text, "clipboard", message or "Copied to clipboard")
  return true
end

function M.paste(text)
  return paste_text(text, nil, true)
end

function M.paste_from_clipboard(after)
  local text, regtype = paste_source()
  local count = vim.v.count1 or 1
  for _ = 1, count do
    if not paste_text(text, regtype, after) then
      return false
    end
  end
  return true
end

local function ordered_range(a, b)
  local srow, scol = a[2], a[3]
  local erow, ecol = b[2], b[3]
  if srow > erow or (srow == erow and scol > ecol) then
    srow, erow = erow, srow
    scol, ecol = ecol, scol
  end
  return srow, scol, erow, ecol
end

local function visual_text(mode)
  local srow, scol, erow, ecol = ordered_range(vim.fn.getpos("v"), vim.fn.getpos("."))
  if srow < 1 or erow < 1 then
    return nil, 0
  end

  local lines = vim.api.nvim_buf_get_lines(0, srow - 1, erow, false)
  if #lines == 0 then
    return nil, 0
  end

  if mode == "V" then
    return table.concat(lines, "\n") .. "\n", #lines
  end

  if mode == "\022" then
    local left = math.min(scol, ecol)
    local right = math.max(scol, ecol)
    for i, line in ipairs(lines) do
      lines[i] = string.sub(line, left, right)
    end
    return table.concat(lines, "\n"), #lines
  end

  if #lines == 1 then
    lines[1] = string.sub(lines[1], scol, ecol)
  else
    lines[1] = string.sub(lines[1], scol)
    lines[#lines] = string.sub(lines[#lines], 1, ecol)
  end

  return table.concat(lines, "\n"), #lines
end

function M.copy_active()
  local mode = vim.fn.mode()
  if mode == "v" or mode == "V" or mode == "\022" then
    local text, count = visual_text(mode)
    set_registers(text, mode)
    if M.store(text, line_count_message("Copied", count)) then
      return
    end
  end

  local lines = vim.api.nvim_buf_get_lines(0, 0, -1, false)
  local text = table.concat(lines, "\n")
  if #lines > 0 then
    text = text .. "\n"
  end
  set_registers(text, "V")
  M.store(text, line_count_message("Copied buffer:", #lines))
end

-- Push a "rio_yank_flash" notification carrying the screen-row/column
-- range the yank covered so the renderer can paint a fading highlight
-- over the actual copied text instead of the whole editor width.
-- Computed off the `'[`/`']` marks (set by every yank/delete) and
-- `screenpos()` so wrapped lines paint every visual row they occupy,
-- not just the buffer-line range.
local function emit_yank_flash()
  local event = vim.v.event or {}
  local s = vim.fn.getpos("'[")
  local e = vim.fn.getpos("']")
  if not s or not e then
    return
  end
  local top = vim.fn.line("w0") or 1
  local bot = vim.fn.line("w$") or top
  -- Clamp to the visible window so a yank that started above the
  -- viewport still paints something sensible — whatever portion is
  -- on-screen.
  local lnum_a = math.max(s[2] or top, top)
  local lnum_b = math.min(e[2] or bot, bot)
  if lnum_b < lnum_a then
    return
  end

  local srow, scol, erow, ecol = ordered_range(s, e)
  local regtype = event.regtype or ""
  if regtype == "V" then
    scol = 1
    ecol = math.max(vim.fn.col({ erow, "$" }) - 1, 1)
  elseif regtype == "\022" then
    local left = math.min(scol, ecol)
    local right = math.max(scol, ecol)
    scol, ecol = left, right
  end

  local start_col = lnum_a == srow and scol or 1
  local end_col = lnum_b == erow and ecol or math.max(vim.fn.col({ lnum_b, "$" }) - 1, 1)
  start_col = math.max(start_col or 1, 1)
  end_col = math.max(end_col or start_col, start_col)

  -- screenpos() returns 1-based screen row/col of (lnum, col) for the
  -- current window. Convert both to 0-based pane-cell coordinates.
  local function screen_pos_for(lnum, col)
    local ok, pos = pcall(vim.fn.screenpos, 0, lnum, col)
    if ok and type(pos) == "table" and (pos.row or 0) > 0 then
      -- screenpos.row is the absolute screen row of the WINDOW
      -- (not the buffer). Convert to 0-based viewport row by
      -- subtracting the window's first screen row.
      local win_row1 = vim.fn.win_screenpos(0)
      local win_top = (win_row1 and win_row1[1] or 1)
      return math.max(pos.row - win_top, 0), math.max((pos.col or 1) - 1, 0)
    end
    return math.max(lnum - top, 0), math.max(col - 1, 0)
  end

  local r0, c0 = screen_pos_for(lnum_a, start_col)
  local r1, c1 = screen_pos_for(lnum_b, end_col)
  if r1 < r0 then
    r0, r1 = r1, r0
    c0, c1 = c1, c0
  end
  pcall(vim.rpcnotify, 1, "rio_yank_flash", r0, r1, c0, c1)
end

function M.on_yank()
  local event = vim.v.event or {}
  if event.operator ~= "y" then
    return
  end

  emit_yank_flash()

  local lines = event.regcontents or {}
  local text = M.lines_to_text(lines, event.regtype)
  set_registers(text, event.regtype)
  M.store(text, line_count_message("Yanked", #lines))
end

function M.setup()
  local group = vim.api.nvim_create_augroup("RioClipboard", { clear = true })
  vim.api.nvim_create_autocmd("TextYankPost", {
    group = group,
    callback = function()
      pcall(M.on_yank)
    end,
  })

  for _, lhs in ipairs({ "<C-c>", "<C-S-c>", "<D-c>" }) do
    pcall(vim.keymap.set, { "n", "x" }, lhs, function()
      M.copy_active()
    end, { silent = true, desc = "Copy through Rio" })
  end

  pcall(vim.keymap.set, "n", "p", function()
    M.paste_from_clipboard(true)
  end, { silent = true, desc = "Paste from system clipboard" })
  pcall(vim.keymap.set, "n", "P", function()
    M.paste_from_clipboard(false)
  end, { silent = true, desc = "Paste from system clipboard" })
end

return M
