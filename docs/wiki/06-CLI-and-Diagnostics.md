# CLI and Diagnostics

## `pstdr-c` Commands

| Command | Description |
| :--- | :--- |
| `pstdr-c fmt <path> [--check]` | Format `.pstdr` files in a file or directory (`--check` exits `1` if unformatted). |
| `pstdr-c fmt --workspace [--check]` | Format all `.pstdr` files in the workspace. |
| `pstdr-c convert --from <in.pstdr> to <out.js> [--ts] [--spec <file>]` | Compile a single `.pstdr` file to `.js` (and `.ts` with `--ts`). |
| `pstdr-c convert --workspace [--spec <file>]` | Compile workspace, wire JSONs, and update `BP/scripts/main.js` & `BP/manifest.json`. |
| `pstdr-c check <path\|--workspace> [--format pretty\|compact\|json] [--spec <file>]` | Validate syntax, semantics, and bindings without emitting files. |
| `pstdr-c watch [--workspace] [--spec <file>]` | Watch `src/` and recompile automatically on changes. |
| `pstdr-c audit --workspace [--format pretty\|compact\|json] [--spec <file>]` | Run `check`, Asset Guard (`E017`), and manifest Audit Gate (`E015`). |
| `pstdr-c explain <CODE>` | Print cause and *Wrong / Right* example for a diagnostic code (e.g. `E001`). |
| `pstdr-c init [pack_name]` | Initialize a new `.pstdr` workspace. |
| `pstdr-c new <script_name>` | Create `src/<script_name>.pstdr` from template. |
| `pstdr-c spec [info\|dump] [--spec <file>]` | Show or dump the active Bedrock specification. |
| `pstdr-c doctor [--spec <file>]` | Print environment, spec, and workspace status. |

**Exit Codes:** `0` = Success · `1` = Compile/Audit error · `2` = Invalid arguments/workspace · `3` = I/O error

---

## Asset Guard (`PSTDR-E017`)

During `convert --workspace` and `audit --workspace`, `pstdr-c` inspects `minecraft:icon` in `BP/items/*.json` and `minecraft:material_instances` in `BP/blocks/*.json`:
- Rejects forbidden aliases containing `/`, `\`, `:`, or file extensions (`.png`, `.tga`, `.jpg`).
- Allows valid vanilla texture aliases (e.g. `"blaze_rod"`, `"snowball"`, `"stone"`) without requiring `RP/`.
- If a custom alias is used, verifies that it is registered in `RP/textures/item_texture.json` or `terrain_texture.json` and that the physical image file exists under `RP/`.

To disable Asset Guard in `pstdr.toml`, set `strict_asset_refs = false` (default: `true`).

---

## Diagnostic Codes (`PSTDR-E001` – `PSTDR-E017`)

`pstdr-c` performs panic-mode recovery and reports multiple syntax and semantic errors in a single pass.

| Code | Cause | Resolution |
| :--- | :--- | :--- |
| **`PSTDR-E001`** | Forbidden keyword (`let`, `var`, `class`, `function`, `nil`, `~=`, `..`, etc.) in safe mode. | Replace with `.pstdr` syntax (`const`, `mut`, `func`, `null`, `!=`, `"{a}{b}"`) or move into `unsafe`. |
| **`PSTDR-E002`** | Assignment (`=`, `+=`, `-=`, `*=`, `/=`) to a `const` variable. | Declare with `mut` instead of `const`. |
| **`PSTDR-E003`** | Reserved `.pstdr` keyword used as an identifier. | Rename the variable, parameter, or function. |
| **`PSTDR-E004`** | `cancel event` used in `func`, `after`, `every`, `on hit`, or `on join`. | Only use `cancel event` inside `on right click` or `on break`. |
| **`PSTDR-E005`** | Context variable (`player`, `target`, etc.) not available in scope. | Declare it or iterate `for p in world.players()` inside `every`/`after`. |
| **`PSTDR-E006`** | `unsafe` used as an anonymous block or expression value. | Declare `unsafe name() { ... }` at top level. |
| **`PSTDR-E007`** | File passed to `bind_target()` not found on disk. | Fix the workspace-relative path. |
| **`PSTDR-E008`** | JSON `"identifier"` differs from the ID used in `.pstdr`. | Match the `"identifier"` in `BP/items/*.json` or `BP/blocks/*.json` with the script ID. |
| **`PSTDR-E009`** | Path traversal (`..`) or absolute path in `bind_target()`. | Use a relative path inside the workspace root. |
| **`PSTDR-E010`** | `every` interval `< 50ms` (`1` tick). | Increase interval to at least `50ms` or `1s`. |
| **`PSTDR-E011`** | Status effect level `< 1`. | Use 1-based levels starting at `1`. |
| **`PSTDR-E012`** | Unclosed `{` brace in block, object, interpolation, or `unsafe`. | Add the matching closing `}`. |
| **`PSTDR-E013`** | Vanilla effect or sound ID not found in active spec. | Check spelling against `data/spec/bedrock_api_latest.json`. |
| **`PSTDR-E014`** | Workspace not detected (`BP/` and `pstdr.toml` missing). | Run in project root, run `pstdr-c init`, or use `convert --from ... to ...`. |
| **`PSTDR-E015`** | Audit Gate failed: invalid `min_engine_version` or BP/RP UUID collision. | Use `[1, 26, 60]` (or `"1.26.60"` for v3) and unique BP/RP UUIDs. |
| **`PSTDR-E016`** | General syntax error (unexpected token, unclosed quote, etc.). | Check the highlighted line and fix grammar. |
| **`PSTDR-E017`** | Invalid texture alias, missing RP registry entry, or missing `.png`/`.tga` file. | Use a valid vanilla alias or register the texture + image file in `RP/`. |

---

## Common Invalid vs. Valid Examples

### `PSTDR-E001` (Forbidden Keyword)
```pstdr
// Invalid
let count = 1
```
```pstdr
// Valid
mut count = 1
```

### `PSTDR-E002` (Constant Reassignment)
```pstdr
// Invalid
on join of player {
    const hp = 10
    hp = 5
}
```
```pstdr
// Valid
on join of player {
    mut hp = 10
    hp = 5
}
```

### `PSTDR-E017` (Texture Reference Without RP)
```json
// Invalid (raw .png path in BP/items/wand.json)
"minecraft:icon": "textures/items/wand.png"
```
```json
// Valid (vanilla alias in BP/items/wand.json)
"minecraft:icon": "blaze_rod"
```

---

> [← Previous: Events and Actions](./05-Events-and-Actions.md) | [Next: Editor Setup →](./07-Editor-Setup.md)
