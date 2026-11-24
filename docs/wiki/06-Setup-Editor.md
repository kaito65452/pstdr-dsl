# Editor Setup (`pstdr-lsp`)

**`pstdr-lsp`** is the Language Server for `.pstdr`. Once connected to your editor, you get:
- **Live red/yellow squiggles** as you type (no need to wait until you run `pstdr-c check`)
- **Built-in fix hints** on hover/code actions
- **One-key formatting** using the same rules as `pstdr-c fmt`

---

## Step 1: Check That `pstdr-lsp` Is in Your `PATH`

Run:

```sh
command -v pstdr-lsp
```

If nothing is printed, make sure `$HOME/.local/bin` is in your `PATH` (see [Installation](./02-Instalasi.md)), or re-run `install.sh` with `pstdr-lsp` checked.

---

## Neovim Setup

Add this to your `~/.config/nvim/init.lua` to recognize `.pstdr` files, start `pstdr-lsp` automatically, and get basic syntax highlighting:

```lua
-- 1. Recognize .pstdr files
vim.filetype.add({
  extension = {
    pstdr = "pstdr",
  },
})

-- 2. Start pstdr-lsp when opening a .pstdr file
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

-- 3. Optional: reuse TypeScript/JavaScript Treesitter highlighting for .pstdr
pcall(vim.treesitter.language.register, "javascript", "pstdr")
```

To format the current `.pstdr` file in Neovim, run `:lua vim.lsp.buf.format()`.

---

## VS Code / Code - OSS Setup

You can connect `pstdr-lsp` in VS Code or Code - OSS using a generic Language Server client extension (such as **Generic LSP Client** or a custom workspace extension) pointing to the `pstdr-lsp` binary:

1. Ensure `pstdr-lsp` is available in your `PATH`.
2. Associate `*.pstdr` files with the `pstdr` language ID in your `settings.json`:
   ```json
   {
     "files.associations": {
       "*.pstdr": "pstdr"
     }
   }
   ```
3. Configure your LSP extension to launch command `pstdr-lsp` over stdio for language ID `pstdr`.

> **Tip:** Even without an editor plugin, you can keep `pstdr-c watch --workspace` running in a terminal split — it automatically re-checks and rebuilds your add-on every time you save a `.pstdr` file.

---

> [← Previous: Troubleshooting Errors](./05-Mengatasi-Error.md) | [Next: Home →](./01-Home.md)
