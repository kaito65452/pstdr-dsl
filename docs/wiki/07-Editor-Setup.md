# Editor Setup (`pstdr-lsp`)

`pstdr-lsp` provides real-time diagnostics, hover information, quick-fix code actions, and document formatting over stdio.

---

## Verify `pstdr-lsp` in `PATH`

```sh
command -v pstdr-lsp
```

If missing, ensure `$HOME/.local/bin` is in `PATH` (see [Installation](./02-Installation.md)).

---

## Neovim (`~/.config/nvim/init.lua`)

```lua
vim.filetype.add({
  extension = {
    pstdr = "pstdr",
  },
})

vim.api.nvim_create_autocmd("FileType", {
  pattern = "pstdr",
  callback = function(args)
    vim.lsp.start({
      name = "pstdr-lsp",
      cmd = { "pstdr-lsp" },
      root_dir = vim.fs.root(args.buf, { "pstdr.toml", "BP", ".git" }),
    })
  end,
})

-- Optional: reuse JavaScript Treesitter highlighting for .pstdr
pcall(vim.treesitter.language.register, "javascript", "pstdr")
```

Format the active buffer with `:lua vim.lsp.buf.format()`.

---

## VS Code / Code - OSS

1. Ensure `pstdr-lsp` is on your `PATH`.
2. Add the file association to `settings.json`:
   ```json
   {
     "files.associations": {
       "*.pstdr": "pstdr"
     }
   }
   ```
3. Configure a generic LSP client extension to spawn `pstdr-lsp` over stdio for language ID `pstdr`.

---

> [← Previous: CLI and Diagnostics](./06-CLI-and-Diagnostics.md) | [Next: Overview →](./01-Overview.md)
