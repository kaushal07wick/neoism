-- Buffer-local `/` search service for rio's command palette.
--
-- The palette in Search mode is a Rust-rendered overlay; instead of
-- routing keystrokes into nvim's native `/`, rio asks lua for the
-- matching lines in the current buffer and renders them as a
-- selectable list. Selecting a row previews the line in the editor
-- pane (cursor + temporary match highlight) so the user can scan results
-- visually before committing. Submit writes nvim's search register
-- and moves with nvim's own search function so post-commit `n` /
-- `N` / `hlsearch` behave as usual.
--
-- All entry points are designed to be called via `nvim_command(lua
-- ...)` from the rust side — no rpcrequest round-trip — so latency
-- stays bounded by the regular nvim_command pipe.

local M = {}

-- Single shared namespace so re-running `preview` blows away the
-- prior selection's highlight before drawing the new one. Buffer-
-- scoped via the buf id passed to `add_highlight`.
local NS = vim.api.nvim_create_namespace("rio_search_preview")

-- Cap the result list so a query that matches every line in a 30k-
-- line buffer doesn't ship 30k entries through the rpc channel.
-- The palette only displays a window anyway.
local MAX_MATCHES = 1000

local function search_ignores_case(query)
  if not vim.o.ignorecase then
    return false
  end
  return not (vim.o.smartcase and query:find("%u"))
end

local function literal_search_pattern(query)
  local case_prefix = search_ignores_case(query) and "\\c" or "\\C"
  return case_prefix .. "\\V" .. vim.fn.escape(query, "\\")
end

local function find_literal(line, query, ignore_case)
  if ignore_case then
    return line:lower():find(query, 1, true)
  end
  return line:find(query, 1, true)
end

-- Trim/normalise a buffer line for display in the palette: collapse
-- tabs to spaces (so the proportional font in the popup doesn't
-- mis-align), and clamp length so a 5kB line doesn't murder the rpc.
local function display_line(s)
  if not s then
    return ""
  end
  s = s:gsub("\t", "    ")
  if #s > 200 then
    s = s:sub(1, 197) .. "..."
  end
  return s
end

function M.search(query)
  M.clear_preview()
  if not query or query == "" then
    pcall(vim.rpcnotify, 1, "rio_search_matches", {})
    return
  end

  local buf = vim.api.nvim_get_current_buf()
  if not vim.api.nvim_buf_is_valid(buf) then
    pcall(vim.rpcnotify, 1, "rio_search_matches", {})
    return
  end

  local lines = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
  local ignore_case = search_ignores_case(query)
  local needle = ignore_case and query:lower() or query
  local matches = {}
  for i, line in ipairs(lines) do
    local start_col = find_literal(line, needle, ignore_case)
    if start_col then
      matches[#matches + 1] = { i, start_col, display_line(line) }
      if #matches >= MAX_MATCHES then
        break
      end
    end
  end
  pcall(vim.rpcnotify, 1, "rio_search_matches", matches)
end

-- Move the cursor to `lnum` and paint a temporary line-highlight on
-- it so the user can see the result behind the popup. Highlight uses
-- the standard `Search` group so it picks up whatever the user's
-- theme assigns (matches what hlsearch would look like).
function M.preview(lnum, col, query)
  if not lnum or lnum <= 0 then
    return
  end
  local buf = vim.api.nvim_get_current_buf()
  if not vim.api.nvim_buf_is_valid(buf) then
    return
  end
  -- Win-set-cursor takes 1-based lnum, 0-based col. Wrap in pcall —
  -- a stale lnum (buffer changed since search ran) shouldn't kill the
  -- preview command.
  pcall(vim.api.nvim_buf_clear_namespace, buf, NS, 0, -1)
  local start_col = math.max((tonumber(col) or 1) - 1, 0)
  pcall(vim.api.nvim_win_set_cursor, 0, { lnum, start_col })
  if query and query ~= "" then
    pcall(
      vim.api.nvim_buf_add_highlight,
      buf,
      NS,
      "Search",
      lnum - 1,
      start_col,
      start_col + #query
    )
  else
    pcall(vim.api.nvim_buf_add_highlight, buf, NS, "Search", lnum - 1, 0, -1)
  end
  -- `zz` centers the line in the window so a series of arrow-down
  -- presses doesn't park results at the very bottom edge.
  pcall(vim.cmd, "normal! zz")
end

-- `backward` mirrors nvim's `?`: the freeform commit jumps to the
-- previous match instead of the next one, and `v:searchforward` flips
-- so a follow-up `n` keeps walking backward (and `N` forward), exactly
-- as if the user had typed `?pattern<CR>` natively.
function M.commit(query, lnum, col, backward)
  if not query or query == "" then
    return
  end
  local buf = vim.api.nvim_get_current_buf()
  if not vim.api.nvim_buf_is_valid(buf) then
    return
  end

  local pattern = literal_search_pattern(query)
  M.clear_preview()
  pcall(vim.fn.setreg, "/", pattern)
  vim.o.hlsearch = true

  if lnum and lnum > 0 then
    local start_col = math.max((tonumber(col) or 1) - 1, 0)
    pcall(vim.api.nvim_win_set_cursor, 0, { lnum, start_col })
    pcall(vim.cmd, "normal! zz")
    vim.v.searchforward = backward and 0 or 1
    return
  end

  local ok, found = pcall(vim.fn.search, pattern, backward and "b" or "")
  if ok and found and found > 0 then
    pcall(vim.cmd, "normal! zz")
  end
  vim.v.searchforward = backward and 0 or 1
end

-- Drop ONLY the rio-side preview highlight, leaving nvim's native
-- `@/` register and `hlsearch` alone. Called after the palette
-- commits a pattern — at that point nvim's own search highlight takes
-- over and `n`/`N` cycle through matches, so we don't want to nuke the
-- highlights the user just asked for.
function M.clear_preview()
  local buf = vim.api.nvim_get_current_buf()
  if vim.api.nvim_buf_is_valid(buf) then
    pcall(vim.api.nvim_buf_clear_namespace, buf, NS, 0, -1)
  end
end

-- Tear down BOTH the rio-side preview highlight and nvim's native
-- hlsearch. Called from rust when the user explicitly bails out of
-- search:
--   * Esc inside the palette without committing
--   * Esc on the editor in Normal mode (the canonical "I'm done with
--     search" gesture, mirroring `:nohlsearch`'s usual binding)
function M.clear()
  M.clear_preview()
  pcall(vim.cmd, "nohlsearch")
end

function M.setup()
  -- No autocmds needed — every entry point is called explicitly from
  -- rust. The require() itself is enough to register the module.
end

return M
