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
