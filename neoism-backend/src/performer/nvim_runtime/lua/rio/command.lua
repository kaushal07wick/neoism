local M = {}

local modal_commands = {
  buffers = true,
  checkhealth = true,
  highlights = true,
  highlight = true,
  jumps = true,
  marks = true,
  messages = true,
  registers = true,
  scriptnames = true,
  syntaxinfo = true,
  version = true,
}

local buffer_output_commands = {
  checkhealth = true,
}

-- LSP actions (definition/hover/references/rename/format/…) are owned by
-- the Rust engine and dispatched from the frontend as EditorLspAction, not
-- via nvim Ex commands — so no `:Definition`-style aliases live here anymore.
local canonical = {
  syntaxinfo = "SyntaxInfo",
}

local function trim(s)
  return (s or ""):gsub("^%s+", ""):gsub("%s+$", "")
end

local function first_word(cmdline)
  return (cmdline:match("^(%S+)") or ""):lower()
end

local function title_for(cmdline)
  return ":" .. cmdline
end

local function current_buffer_text(previous_buf)
  local current = vim.api.nvim_get_current_buf()
  if current == previous_buf then
    return ""
  end

  local ok, lines = pcall(vim.api.nvim_buf_get_lines, current, 0, -1, false)
  if not ok or not lines or #lines == 0 then
    return ""
  end
  return table.concat(lines, "\n")
end

local function close_command_buffer(previous_win, previous_buf)
  local current = vim.api.nvim_get_current_buf()
  pcall(vim.api.nvim_set_current_win, previous_win)
  if current ~= previous_buf and vim.api.nvim_buf_is_valid(current) then
    pcall(vim.api.nvim_buf_delete, current, { force = true })
  end
end

local function exec_capture(cmdline)
  local ok_execute, execute_output = pcall(vim.fn.execute, cmdline)
  if ok_execute then
    return true, execute_output or ""
  end

  if vim.api.nvim_exec2 then
    local ok, result = pcall(vim.api.nvim_exec2, cmdline, { output = true })
    if ok then
      return true, result and result.output or ""
    end
  end

  local ok, output = pcall(vim.api.nvim_exec, cmdline, true)
  if ok then
    return true, output or ""
  end
  return false, tostring(execute_output or output)
end

local function run_modal_command(cmdline)
  local key = first_word(cmdline)
  if key == "syntaxinfo" then
    require("rio.treesitter").info()
    return
  end

  local previous_win = vim.api.nvim_get_current_win()
  local previous_buf = vim.api.nvim_get_current_buf()
  local ok, output = exec_capture(cmdline)
  if ok then
    local buffer_text = current_buffer_text(previous_buf)
    if buffer_output_commands[key] and trim(buffer_text) ~= "" then
      output = buffer_text
    elseif trim(output) == "" then
      output = buffer_text
    end
    close_command_buffer(previous_win, previous_buf)
  end

  output = trim(output)
  if not ok then
    pcall(vim.rpcnotify, 1, "rio_modal", title_for(cmdline), output, "error")
    return
  end

  if output == "" then
    pcall(vim.rpcnotify, 1, "rio_notify", "No output for :" .. cmdline, "info")
    return
  end

  pcall(vim.rpcnotify, 1, "rio_modal", title_for(cmdline), output, "info")
end

function M.run(cmdline)
  cmdline = trim(cmdline)
  if cmdline == "" then
    return
  end

  local key = first_word(cmdline)
  if canonical[key] and not cmdline:find("%s") then
    cmdline = canonical[key]
    key = first_word(cmdline)
  end

  if modal_commands[key] then
    run_modal_command(cmdline)
    return
  end

  local ok, err = pcall(vim.cmd, cmdline)
  if not ok then
    pcall(vim.rpcnotify, 1, "rio_modal", title_for(cmdline), tostring(err), "error")
  end
end

return M
