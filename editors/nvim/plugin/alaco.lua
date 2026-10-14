if vim.g.loaded_alaco_nvim then
  return
end
vim.g.loaded_alaco_nvim = true

local executable = function()
  return vim.g.alaco_executable or "alaco"
end

local function run(action)
  local file = vim.api.nvim_buf_get_name(0)
  if file == "" then
    vim.notify("Save the Alaco file before running a command", vim.log.levels.ERROR)
    return
  end
  if vim.bo[0].filetype ~= "alaco" and not file:match("%.aco$") then
    vim.notify("Alaco commands require an open .aco file", vim.log.levels.ERROR)
    return
  end
  vim.cmd("botright new")
  vim.fn.termopen({ executable(), action, file })
  vim.cmd("startinsert")
end

vim.api.nvim_create_user_command("AlacoCheck", function() run("check") end, {})
vim.api.nvim_create_user_command("AlacoBuild", function() run("build") end, {})
vim.api.nvim_create_user_command("AlacoRun", function() run("run") end, {})

-- Use Neovim's built-in LSP client; no external LSP plugin is required.
vim.api.nvim_create_autocmd("FileType", {
  group = vim.api.nvim_create_augroup("alaco_lsp", { clear = true }),
  pattern = "alaco",
  callback = function(args)
    local root = vim.fs.root(args.buf, { "Alaco.toml", ".git" }) or vim.fn.getcwd()
    vim.lsp.start({
      name = "alaco",
      cmd = { executable(), "lsp" },
      root_dir = root,
      capabilities = vim.lsp.protocol.make_client_capabilities(),
    })
  end,
})

vim.api.nvim_create_autocmd("LspAttach", {
  group = vim.api.nvim_create_augroup("alaco_lsp_keymaps", { clear = true }),
  callback = function(args)
    local client = vim.lsp.get_client_by_id(args.data.client_id)
    if not client or client.name ~= "alaco" then
      return
    end
    vim.bo[args.buf].omnifunc = "v:lua.vim.lsp.omnifunc"
    local opts = { buffer = args.buf, silent = true }
    vim.keymap.set("n", "gd", vim.lsp.buf.definition, opts)
    vim.keymap.set("n", "gr", vim.lsp.buf.references, opts)
    vim.keymap.set("n", "K", vim.lsp.buf.hover, opts)
    vim.keymap.set("n", "<leader>rn", vim.lsp.buf.rename, opts)
    vim.keymap.set("n", "<leader>ca", vim.lsp.buf.code_action, opts)
    vim.keymap.set("n", "[d", vim.diagnostic.goto_prev, opts)
    vim.keymap.set("n", "]d", vim.diagnostic.goto_next, opts)
    vim.keymap.set("n", "<leader>f", function()
      vim.lsp.buf.format({ bufnr = args.buf })
    end, opts)
  end,
})
