local M = {}

function M.setup()
  require("rio.options").setup()
  require("rio.events").setup()
  require("rio.theme").setup()
  require("rio.treesitter").setup()
  require("rio.clipboard").setup()
  require("rio.completion").setup()
  require("rio.changesigns").setup()
  require("rio.search").setup()
  require("rio.minimap").setup()
  require("rio.command")
end

return M
