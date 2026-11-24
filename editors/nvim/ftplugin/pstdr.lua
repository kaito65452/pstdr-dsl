vim.bo.commentstring = "// %s"
vim.bo.shiftwidth = 4
vim.bo.tabstop = 4
vim.bo.expandtab = true

if vim.fn.executable("pstdr-lsp") == 1 then
  local root_dir = vim.fs.root(0, { "pstdr.toml", "BP", ".git" })
  vim.lsp.start({
    name = "pstdr-lsp",
    cmd = { "pstdr-lsp" },
    root_dir = root_dir,
  })
end
