# Installation

You can install `pstdr-c` (and optionally `pstdr-lsp` and `PackStudio`) with a single terminal command on Linux and macOS.

---

## Quick 1-Line Install

```sh
curl -fsSL https://raw.githubusercontent.com/kaito65452/pstdr-dsl/main/install.sh | sh
```

### What Do the `curl` Flags Mean?
- `-f`: Fail cleanly if the download URL returns an HTTP error (such as 404).
- `-s`: Silent mode — hides the download progress bar so it doesn't clutter your screen.
- `-S`: Still show an error message if something actually goes wrong.
- `-L`: Follow redirects automatically to reach the script.

---

## Using the Interactive Menu

When you run the installer in a terminal, it checks for `git` and `cargo` (offering to install Rust via `rustup` if missing), then shows the component selector:

```text
Select components to install:
  1) [x] pstdr-c      (Compiler & CLI — required)
  2) [x] pstdr-lsp    (Language Server for editors)
  3) [ ] PackStudio   (Optional GUI/companion app)

Type 2 or 3 to toggle, Enter to continue, or q to cancel:
```

- Press **`2`** + **Enter** to toggle `pstdr-lsp` on or off.
- Press **`3`** + **Enter** to toggle `PackStudio` on or off.
- Press **Enter** directly to accept the current selection and start building.
- Press **`q`** + **Enter** to cancel.

---

## Non-Interactive Install (Scripts / CI)

If you want to skip all prompts, set `PSTDR_YES=1`:

```sh
PSTDR_YES=1 PSTDR_LSP=1 PSTDR_STUDIO=0 curl -fsSL https://raw.githubusercontent.com/kaito65452/pstdr-dsl/main/install.sh | sh
```

Available environment variables:
- `PSTDR_YES=1` — Accept defaults without prompting.
- `PSTDR_LSP=1` or `0` — Include or skip `pstdr-lsp` (default: `1`).
- `PSTDR_STUDIO=1` or `0` — Include or skip `PackStudio` (default: `0`).
- `PSTDR_INSTALL_DIR` — Custom binary directory (default: `$HOME/.local/bin`).
- `PSTDR_REPO_URL` / `PSTDR_BRANCH` — Custom git repository or branch.

---

## Setting Up Your `PATH`

If the installer warns that `$HOME/.local/bin` is not in your `PATH`, add it to your shell config:

**Bash (`~/.bashrc`) or Zsh (`~/.zshrc`):**
```sh
export PATH="$HOME/.local/bin:$PATH"
```
Reload your shell config (for example `source ~/.bashrc` or `source ~/.zshrc`).

**Fish (`~/.config/fish/config.fish`):**
```fish
fish_add_path $HOME/.local/bin
```

---

## Verify Installation

Run these two commands to confirm everything is ready:

```sh
pstdr-c --help
pstdr-c doctor
```

`pstdr-c doctor` shows the active Bedrock specification (`26.60`, `@minecraft/server 2.11.0`) and checks your current workspace.

---

> [← Previous: Home](./01-Home.md) | [Next: Your First Add-On →](./03-Addon-Pertama.md)
