local M = {}

local enabled = false

local function palette()
  local ok, theme = pcall(require, "rio.theme")
  if ok and theme.palette then
    return theme.palette()
  end

  return {
    muted = "#565c64",
    info = "#61afef",
    error = "#e06c75",
    keyword = "#c678dd",
    statement = "#e06c75",
    func = "#61afef",
    type = "#e5c07b",
    string = "#98c379",
    property = "#e06c75",
    constructor = "#56b6c2",
  }
end

local function chunk_style()
  local p = palette()
  return {
    { fg = p.info or p.func or "#61afef" },
    { fg = p.error or "#e06c75" },
  }
end

local function indent_style()
  local p = palette()
  return {
    { fg = p.muted or "#565c64" },
  }
end

local function node_type_styles()
  local p = palette()
  return {
    ["function"] = { fg = p.func },
    ["method"] = { fg = p.func },
    ["arrow_function"] = { fg = p.func },
    ["^if"] = { fg = p.keyword },
    ["else"] = { fg = p.keyword },
    ["^match"] = { fg = p.keyword },
    ["^switch"] = { fg = p.keyword },
    ["^for"] = { fg = p.type },
    ["^while"] = { fg = p.type },
    ["^loop"] = { fg = p.type },
    ["^try"] = { fg = p.string },
    ["catch"] = { fg = p.string },
    ["except"] = { fg = p.string },
    ["class"] = { fg = p.constructor or p.type },
    ["struct"] = { fg = p.constructor or p.type },
    ["impl"] = { fg = p.constructor or p.type },
  }
end

local function config()
  return {
    chunk = {
      enable = true,
      notify = false,
      use_treesitter = true,
      delay = 120,
      duration = 120,
      style = chunk_style,
      node_type_styles = node_type_styles(),
    },
    indent = {
      enable = true,
      notify = false,
      chars = { "│" },
      delay = 80,
      style = indent_style,
    },
  }
end

local function apply()
  local ok, hlchunk = pcall(require, "hlchunk")
  if not ok or type(hlchunk.setup) ~= "function" then
    pcall(function()
      require("rio.events").notify("hlchunk.nvim failed to load: " .. tostring(hlchunk), "warn")
    end)
    return false
  end

  local setup_ok, err = pcall(hlchunk.setup, config())
  if not setup_ok then
    pcall(function()
      require("rio.events").notify("hlchunk.nvim setup failed: " .. tostring(err), "warn")
    end)
    return false
  end

  return true
end

function M.setup()
  enabled = apply()
end

function M.refresh()
  if enabled then
    apply()
  end
end

return M
