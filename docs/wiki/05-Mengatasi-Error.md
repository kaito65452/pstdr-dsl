# Troubleshooting Errors

`pstdr-c` is designed to catch mistakes early — and thanks to **multi-error reporting**, `pstdr-c check` and `pstdr-c convert` show **all errors in your file at once** instead of stopping at the first one.

---

## How to Read an Error Message

```text
error[PSTDR-E001]: Forbidden keyword 'let' found in safe mode.
  --> src/frost_scythe.pstdr:1:1
  help: Replace with its .pstdr equivalent (const/mut/func/null/{ ... }) or move into an 'unsafe' function.

error[PSTDR-E002]: Cannot reassign constant variable 'y'.
  --> src/frost_scythe.pstdr:4:5
  help: Change the declaration on line 3 to 'mut y = ...' if its value needs to change.
```

Every error tells you:
1. **Error code** (e.g. `PSTDR-E001`)
2. **Exact file, line, and column** (`src/frost_scythe.pstdr:1:1`)
3. **A direct fix** (`help: ...`)

Want a longer explanation with a *Wrong / Right* example? Run:

```sh
pstdr-c explain E001
```

---

## Common Mistakes to Watch Out For

### 1. `PSTDR-E001` — Using `let`, `var`, `function`, or `class`
`.pstdr` uses `const` (read-only) and `mut` (changeable) instead of JavaScript's `let` or `var`.
- **Wrong:** `let speed = 5`
- **Right:** `mut speed = 5`

### 2. `PSTDR-E002` — Changing a `const` Variable
Variables declared with `const` cannot be modified later.
- **Wrong:** `const hp = 10` followed by `hp = 5`
- **Right:** `mut hp = 10` followed by `hp = 5`

### 3. `PSTDR-E017` — Custom or Invalid Textures Without `RP/`
Minecraft Bedrock crashes or renders broken icons if a Behavior Pack item/block uses a custom texture name without a matching Resource Pack (`RP/`).
- If you **don't have an `RP/` folder**, use a valid vanilla texture alias like `"snowball"` or `"diamond"`.
- Never put `.png`, `.tga`, folders (`textures/...`), or `minecraft:` inside `minecraft:icon`.
- If you **do** use a custom icon like `"frost_scythe"`, register it in `RP/textures/item_texture.json` and make sure the `.png` file exists on disk.

---

## Full Error Code Reference (`PSTDR-E001` – `PSTDR-E017`)

| Code | Meaning | Quick Fix |
| :--- | :--- | :--- |
| **`PSTDR-E001`** | Forbidden keyword (`let`, `var`, `class`, `new`, `function`, `nil`, `~=`, `..`, etc.) in safe mode | Use `.pstdr` syntax (`const`, `mut`, `func`, `null`, `!=`, `"{a}{b}"`) or move into `unsafe`. |
| **`PSTDR-E002`** | Reassigning a `const` variable | Change `const` to `mut` if the value needs to change. |
| **`PSTDR-E003`** | Reserved `.pstdr` keyword used as a name | Pick a different variable, parameter, or function name. |
| **`PSTDR-E004`** | `cancel event` used outside a cancellable event | Only use `cancel event` inside `on right click` or `on break`. |
| **`PSTDR-E005`** | Variable or context (`player`, `target`) missing in scope | Declare it first, or iterate `for p in world.players()` inside `every`/`after`. |
| **`PSTDR-E006`** | `unsafe` used inside a block or as an expression | Declare `unsafe name() { ... }` at the top level of the file. |
| **`PSTDR-E007`** | File passed to `bind_target(...)` does not exist | Check the relative path from your project root (e.g. `"BP/items/my_item.json"`). |
| **`PSTDR-E008`** | Item/block `"identifier"` in JSON doesn't match the script | Make sure the JSON `"identifier"` matches the ID in your `.pstdr` event. |
| **`PSTDR-E009`** | Absolute path or `..` path traversal in `bind_target(...)` | Use a relative path inside the workspace (no `..` or leading `/`). |
| **`PSTDR-E010`** | `every` interval is smaller than 1 tick (`50ms`) | Use at least `50ms` or `1s` so the Bedrock tick loop doesn't freeze. |
| **`PSTDR-E011`** | Status effect level is `0` or negative | Use 1-based levels starting at `1` (`apply speed 1 ...`). |
| **`PSTDR-E012`** | Opening `{` has no closing `}` | Add the missing closing `}` brace. |
| **`PSTDR-E013`** | Unknown vanilla effect or sound ID | Check spelling against `data/spec/bedrock_api_latest.json`. |
| **`PSTDR-E014`** | Workspace not detected (`BP/` and `pstdr.toml` missing) | Run from your project root, run `pstdr-c init`, or use `convert --from ... to ...`. |
| **`PSTDR-E015`** | Audit Gate failed (`min_engine_version` or UUID collision) | Ensure `min_engine_version` starts with `1` (e.g. `[1, 26, 60]`) and BP/RP UUIDs differ. |
| **`PSTDR-E016`** | General syntax error (missing token, unclosed quote, etc.) | Check the highlighted line for typos or missing punctuation. |
| **`PSTDR-E017`** | Invalid texture alias or missing Resource Pack image file | Use a valid vanilla alias (e.g. `"snowball"`) or add `RP/textures/...` + `.png`. |

---

> [← Previous: Code Recipes](./04-Resep-Kode.md) | [Next: Editor Setup →](./06-Setup-Editor.md)
