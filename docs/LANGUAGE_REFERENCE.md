# pstdr-dsl Language Reference

> Authoritative in-repo specification for **pstdr-dsl** (Waves 1–3C), derived directly from `crates/pstdr-lang/src/` and `data/spec/`.
> Supported profiles: **Modern 2.x** (Bedrock `26.60`, `@minecraft/server 2.11.0`, `@minecraft/server-ui 2.0.0`) and **Legacy 1.x LTS** (Bedrock `1.21.50+`, `@minecraft/server 1.15.0`–`1.19.x`, `@minecraft/server-ui 1.3.0`).
> For a compact LLM/agent prompt sheet, see [llms.txt](../llms.txt).

---

## 1. File Directives & Multi-Target `ApiProfile`

```pstdr
@addon(target = "26.60", api = "2.11.0")
bind_target("BP/items/frost_blade.json")
```

- **`@addon(target = "<ver>", api = "<ver>")`**
  - Optional; at most **one** per file (duplicate `@addon` fails with `PSTDR-E016`).
  - Defaults to `target = "26.60"`, `api = "2.11.0"` (loaded from `data/spec/bedrock_api_latest.json`).
  - Can also be set workspace-wide in `pstdr.toml` under `[pack]` or `[bedrock]` (`target = "..."`, `api = "..."`).
  - Only `target` and `api` attributes are supported, and values must be double-quoted string literals.
- **`bind_target("<relative_path>")`**
  - Optional; multiple directives allowed.
  - Relative path from workspace root to a `BP/items/*.json` or `BP/blocks/*.json` file. No `..` or absolute paths (`PSTDR-E009`).
  - Compiler checks the file exists (`PSTDR-E007`), wires custom components (V1 array on `1.21.50`, V2 direct key on `1.21.90+`), and verifies that the JSON `"identifier"` matches the item/block ID used in your script (`PSTDR-E008`).

### `ApiProfile` Matrix (`Modern2x` vs `Legacy1x`)

`@minecraft/server` versions below `1.15.0` are **not supported** and fail with `PSTDR-E016` directing you to `1.15.0+` (Legacy 1.x LTS) or `2.11.0` (Modern 2.x).

| Capability | Modern 2.x (`2.0.0`–`2.11.x`, default) | Legacy 1.x LTS (`1.15.0`–`1.19.x`) |
| :--- | :--- | :--- |
| Bundled Spec | `data/spec/bedrock_api_latest.json` | `data/spec/bedrock_api_v1_legacy.json` |
| Default Target & `min_engine_version` | `"26.60"` (`[1, 26, 60]`) | `"1.21.50"` (`[1, 21, 50]`) |
| Component JSON `format_version` | `"1.21.90"` (Custom Components V2) | `"1.21.50"` (Custom Components V1) |
| Component Registration Hook | `system.beforeEvents.startup` | `world.beforeEvents.worldInitialize` |
| `command` / `run_fn` / `.run_command()` | `.runCommand(...)` | `.runCommandAsync(...)` |
| `on cmd` (`customCommandRegistry`) | **Supported (Stable)** | **Rejected (`PSTDR-E016`)** — use `on chat` or upgrade to `2.11.0` |
| `on chat` (`world.beforeEvents.chatSend`) | **Beta-only (`2.11.0-beta`, `PSTDR-E018`)** | **Stable** (allowed without `-beta`) |
| `@minecraft/server-ui` (conditional) | `"2.0.0"` (only injected if `ui` is used) | `"1.3.0"` (only injected if `ui` is used) |
| `runCommandAsync` in safe/`unsafe` | **Rejected (`PSTDR-E016`)** | Supported |

---

## 2. Top-Level Declarations

| Construct | Syntax | Description |
| :--- | :--- | :--- |
| Constant | `const <name> = <expr>` | Immutable binding; reassignment triggers `PSTDR-E002`. |
| Mutable Variable | `mut <name> = <expr>` | World/module-scoped mutable binding. Supports `=`, `+=`, `-=`, `*=`, `/=`. |
| Persistent State | `state <name>[<owner>] = <default>` | Per-entity Dynamic Property (`pstdr:<name>`). |
| Function | `func <name>(<p1>, <p2>) { ... }` | Validated `.pstdr` function. |
| Inline `.mcfunction` *(3A)* | `mcfunction "<path>" { ... }` | Emits raw Bedrock commands to `BP/functions/<path>.mcfunction` (`//` comments stripped). |
| Repeating Timer | `every <duration> { ... }` | Compiles to `system.runInterval(..., ticks)`. Minimum `50ms` (`PSTDR-E010`). |
| One-Shot Timer | `after <duration> { ... }` | Compiles to `system.runTimeout(..., ticks)`. |
| Event Handler | `on <event> { ... }` | Subscribes to Bedrock `world`/`system` events, custom commands, or custom components. |
| Unsafe Function | `unsafe <name>(<p1>, ...) { ... }` | Top-level raw JS/TS escape hatch. |
| Unsafe Variable | `unsafe const\|mut\|var\|let <name> = ...;` | Single-line raw JS/TS declaration at top level (`mut` emits `let`). |

### Multiplayer State Rule

Always use `state <key>[player] = <default>` for per-player cooldowns, scores, timers, and flags. Never store per-player values in a single global `mut` variable.

---

## 3. Complete Event Catalog (Wave 1–3C)

| Event | Exact Syntax | Scope Variables | Underlying Bedrock Hook |
| :--- | :--- | :--- | :--- |
| **Custom Command** *(3B)* | `on cmd "<ns>:<name>" description "<desc>" [permission any\|gamemakers\|admin\|host] [(<p>: <type>, <opt>?: <type>)] [by <actor>] { ... }` | params, `[<actor>]` | `system.beforeEvents.startup` -> `customCommandRegistry.registerCommand` (**Stable `2.x`**; rejected on `1.x`) |
| **Chat Send** | `on chat <msg> by <actor> { ... }` | `<msg>`, `<actor>` | `world.beforeEvents.chatSend` (**`2.x` Beta-only `E018`**, **`1.x` Stable**) |
| **Right Click** | `on right click with <item> by <actor> { ... }` | `<actor>` | `afterEvents.itemUse` / `beforeEvents.itemUse` (or item component `onUse`) |
| **Hit Entity** | `on hit <target> with <item> by <actor> { ... }` | `<target>`, `<actor>` | `afterEvents.entityHitEntity` (or item component `onHitEntity`) |
| **Consume Item** | `on consume <item> by <actor> { ... }` | `<actor>` | `afterEvents.itemCompleteUse` (or item component `onConsume`) |
| **Item Release** *(3B)* | `on item_release with <item> by <actor> { ... }` | `<actor>`, `item`, `use_duration` | `world.afterEvents.itemReleaseUse` |
| **Break Block** | `on break of <block_id> by <actor> { ... }` | `<actor>`, `block` | `afterEvents.playerBreakBlock` / `beforeEvents.playerBreakBlock` (or `onPlayerBreak`) |
| **Place Block** *(3B)* | `on place of <block> by <actor> { ... }` | `<block>`, `<actor>` | `afterEvents.playerPlaceBlock` / `beforeEvents.playerPlaceBlock` (Cancellable) |
| **Block Interact** | `on interact of <block_id> by <actor> { ... }` | `<actor>`, `block` | Block component `onPlayerInteract` or `playerInteractWithBlock` (Cancellable) |
| **Block Tick** | `on tick of <block_id> { ... }` | `block`, `dimension` | Custom block component `onTick` (requires matching `BP/blocks/*.json`, `E007`) |
| **Interact Entity** *(3B)* | `on interact_entity of <target> by <actor> { ... }` | `<target>`, `<actor>` | `afterEvents.playerInteractWithEntity` / `beforeEvents.playerInteractWithEntity` (Cancellable) |
| **Player Join** | `on join of <actor> { ... }` | `<actor>` | `world.afterEvents.playerSpawn` (filtered to `ev.initialSpawn`) |
| **Player Leave** | `on leave of <actor> { ... }` | `<actor>` | `world.beforeEvents.playerLeave` (auto `system.run` for mutations) |
| **Dimension Change** *(3B)* | `on dimension_change of <player> { ... }` | `<player>`, `from_dimension`, `to_dimension`, `from_location`, `to_location` | `world.afterEvents.playerDimensionChange` |
| **Entity Hurt** | `on hurt <target> [by <attacker>] { ... }` | `<target>`, `[<attacker>]` | `world.afterEvents.entityHurt` |
| **Entity Die** | `on die of <target> [by <killer>] { ... }` | `<target>`, `[<killer>]` | `world.afterEvents.entityDie` |
| **Health Change** *(3B)* | `on health_change of <entity> { ... }` | `<entity>`, `old_value`, `new_value` | `world.afterEvents.entityHealthChanged` |
| **Projectile Hit** | `on projectile hit <target> [with <proj_id>] [by <shooter>] { ... }` | `<target>`, `[<shooter>]`, `projectile` | `world.afterEvents.projectileHitEntity` |
| **Projectile Hit Block** *(3B)* | `on projectile_hit_block of <block> [by <shooter>] { ... }` | `<block>`, `projectile`, `<shooter>` (default `shooter`) | `world.afterEvents.projectileHitBlock` |
| **Entity Hit Block** *(3B)* | `on entity_hit_block of <block> by <attacker> { ... }` | `<block>`, `<attacker>` | `world.afterEvents.entityHitBlock` |
| **Explosion** *(3B)* | `on explosion { ... }` | `source`, `impacted_blocks` | `afterEvents.explosion` / `beforeEvents.explosion` (Cancellable) |
| **Entity Spawn** | `on spawn of <entity> [type <type_id>] { ... }` | `<entity>` | `world.afterEvents.entitySpawn` |
| **Entity Load** *(3B)* | `on entity_load of <entity> { ... }` | `<entity>` | `world.afterEvents.entityLoad` |
| **Entity Remove** *(3B)* | `on entity_remove of <entity> { ... }` | `<entity>` | `world.afterEvents.entityRemove` |
| **Script Event** *(3A)* | `on script_event "<namespace>:<id>" { ... }` | `event`, `source`, `source_block` | `system.afterEvents.scriptEventReceive` (static `"ns:id"` required, `E016`) |
| **Entity Trigger** *(3A)* | `on entity_trigger "<event_id>" [for "<entity_type>"] { ... }` | `entity`, `event_id` | `world.afterEvents.dataDrivenEntityTrigger` |

### Custom Commands (`on cmd`, Wave 3B)

```pstdr
on cmd "my_pack:guess" description "Submit a guess" permission any (word: string, tries?: int) by player {
    send "You guessed: {word}" to player
}
```

- Command ID must be a static string/const with a namespace colon (`"<ns>:<name>"`, `PSTDR-E016`).
- Permissions: `any` (default), `gamemakers`, `admin`, `host`.
- Parameter types: `string`, `int`, `float`, `bool`, `player`. Optional parameters (`name?: type`) must come after all required parameters (`PSTDR-E016`).
- In multi-file workspaces with custom components or multiple command files, commands are emitted to `BP/scripts/generated/commands/<stem>.js` and merged into `_registry.js`'s single `system.beforeEvents.startup.subscribe(...)` callback.

### `beforeEvents` Read-Only Safety

`on leave` and `on chat` always use `world.beforeEvents`; `on right click`, `on break`, `on place`, `on interact`, `on interact_entity`, and `on explosion` automatically switch to `world.beforeEvents` when they contain `cancel event`. World-mutating actions inside `beforeEvents` (including `ui` forms, `score` writes, `command`, `run_fn`, `state` writes, etc.) are automatically deferred with `system.run(() => { ... })`.

---

## 4. Statements, Native UI & Scoreboards

Semicolons (`;`) are optional at the end of statements.

### Action Statements

| Action | Syntax | Compiled Output / Notes |
| :--- | :--- | :--- |
| **Cancel Event** | `cancel event` | `ev.cancel = true;`. Only valid in `on right click`, `on break`, `on place`, `on interact`, `on interact_entity`, `on explosion`, `on chat` (`E004`). |
| **Send Message** | `send <msg> to <target>` | `<target>.sendMessage(<msg>);` (`<target>` = player or `world`). |
| **Apply Effect** | `apply <effect> <level> to <target> for <duration>` | `<target>.addEffect("<effect>", <ticks>, { amplifier: <level> - 1 });` (1-based `level >= 1`, `E011`; validated by `E013`). |
| **Damage / Ignite** | `damage <target> by <amount>` / `ignite <target> for <duration>` | `applyDamage(...)` / `setOnFire(<seconds>, true)`. |
| **Spawn Particle** | `spawn particle <id> at <entity_or_loc> [in <dim>]` | `<dim>.spawnParticle(<id>, <loc>);`. |
| **Run Command** *(3A)* | `command [on <target>] <cmd_expr>` | `.runCommand(<cmd>)` (`2.x`) / `.runCommandAsync(<cmd>)` (`1.x`); defaults to overworld. |
| **Run `.mcfunction`** *(3A)* | `run_fn [on <target>] <path_expr>` | Runs `function <path>`; validates `BP/functions/<path>.mcfunction` or inline `mcfunction` exists (`E017`, `E009`). |

### Native Scoreboards (Wave 3B)

```pstdr
score["kills"][player] = 0
score["kills"][player] += 1
score["kills"][player] -= 1
const k = score["kills"][player]
scoreboard show "kills" title "Top Kills" slot sidebar
scoreboard hide slot belowname
```

- Display slots: `sidebar` (`DisplaySlotId.Sidebar`), `list` (`DisplaySlotId.List`), `belowname` (`DisplaySlotId.BelowName`).
- Objectives are auto-created on first access via a deduplicated `__pstdr_get_or_create_objective` helper.

### Native UI Forms (Wave 3B)

`@minecraft/server-ui` (`"2.0.0"` on Modern `2.x`, `"1.3.0"` on Legacy `1.x LTS`) is automatically injected into `BP/manifest.json` **only** when at least one `ui` block is present.

```pstdr
ui action "Class Menu" body "Choose your role:" to player {
    button "Warrior" icon "textures/items/iron_sword" {
        player.give_item("minecraft:iron_sword", 1)
    }
    button "Archer" {
        player.give_item("minecraft:bow", 1)
    }
}

ui message "Confirm" body "Proceed?" to player {
    button1 "Yes" { send "Accepted" to player }
    button2 "No" { send "Declined" to player }
}

ui modal "Settings" to player {
    text_field "Nickname" placeholder "Enter..." default "Steve" as nick
    toggle "Hardcore" default false as hard
    slider "Difficulty" min 1 max 5 step 1 default 2 as diff
    dropdown "Role" options ["Warrior", "Archer"] default 0 as role
    on submit {
        send "{nick}:{hard}:{diff}:{role}" to player
    }
}
```

---

## 5. Literals, Units, Operators & Predicates

- **Durations (EXACT):** `ms`, `s`, `m` ONLY (`50ms` = 1 tick, `3s` = 60 ticks, `2m` = 2400 ticks). **There is NO `t` tick suffix** — write `50ms` for 1 tick.
- **Distances:** `5b`, `5blocks`.
- **Strings:** Double-quoted `"..."` only, with `{expr}` interpolation (`"HP: {target.hp}"`). Escapes: `\n`, `\r`, `\t`, `\\`, `\"`, `\{`, `\}`. Single quotes and backticks are rejected outside `unsafe` (`E016`).
- **Operators (Lowest to Highest):** `?:` -> `or`/`||` -> `and`/`&&` -> `not` -> `== != < <= > >= is [not] in within` -> `??` -> `+ -` -> `* / %` -> `- !` -> `. ?. [] ()`.
- **`is` / `is not` Predicates:** `sneaking`, `sprinting`, `swimming`, `flying`, `sleeping`, `on_ground`, `in_water`, `holding <item_id>`.

---

## 6. Built-in Globals, Validation Guards & `snake_case` Mappings

- **Globals:** `world`, `system`, `console`, `Math`, `JSON`, `String`, `Number`, `Boolean`, `Array`, `Object`, `Date`, `parseInt`, `parseFloat`, `isNaN`.
- **Hallucination Guards (`PSTDR-E016`):**
  - Unknown `world.*()` and `system.*()` methods (e.g. `world.get_time()`) are rejected with suggestions.
  - Synthesized helper arities are enforced: `title(msg)` (1 arg), `subtitle(msg)` (1 arg), `actionbar(msg)` (1 arg), `give_item(id, amount?, name?)` (1–3 args).
  - `runCommandAsync` is rejected on Modern `2.x`.
- **Mapped `world` Methods:** `players()` / `get_all_players()`, `get_players(opts)`, `send_message(msg)`, `dimension(id)` / `get_dimension(id)`, `play_sound(id, loc)`, `play_music(...)`, `queue_music(...)`, `stop_music()`, `get_entity(id)`, `get_day()`, `get_moon_phase()`, `get_time_of_day()`, `set_time_of_day(t)`, `get_absolute_time()`, `set_absolute_time(t)`, `get_default_spawn_location()`, `set_default_spawn_location(loc)`, `get_dynamic_property(k)`, `set_dynamic_property(k, v)`, `get_dynamic_property_ids()`, `clear_dynamic_properties()`.
- **Mapped `system` Methods & Properties:** `current_tick` (`currentTick`), `run(fn)`, `run_timeout(fn, t)`, `run_interval(fn, t)`, `clear_run(id)`, `run_job(g)`, `clear_job(id)`, `send_script_event(id, msg)`, `wait_ticks(n)`.
- **`snake_case` String & Array Helpers:** `lower()` (`toLowerCase`), `upper()` (`toUpperCase`), `trim()`, `starts_with(s)`, `ends_with(s)`, `contains(x)` (`includes`), `replace(a, b)`, `split(sep)`, `join(sep)`, `push(x)`.
- **Entity/Player Properties:** `name_tag`, `type_id`, `is_sneaking`, `is_sprinting`, `is_swimming`, `is_flying`, `is_sleeping`, `is_on_ground`, `is_in_water`, `is_climbing`, `is_falling`, `is_valid`, `selected_slot`, `head_location`, `view_direction`, `hp`, `max_hp`, `holding` / `mainhand` (item sub-props: `.id`, `.type_id`, `.name_tag`).
- **Entity/Player/Dimension Methods:** `actionbar(msg)`, `title(msg)`, `subtitle(msg)`, `send_message(msg)`, `play_sound(id)`, `add_tag`, `remove_tag`, `has_tag`, `get_tags`, `apply_damage`, `apply_impulse`, `apply_knockback`, `run_command`, `add_effect`, `remove_effect`, `set_on_fire`, `extinguish_fire`, `spawn_particle`, `spawn_entity`, `spawn_item`, `give_item`, `get_score`, `set_score`, `add_score`, `holding(id)`, `get_entities`, `get_players`, `teleport`, `kill`.

---

## 7. CLI Workflow & Editor LSP Setup

### CLI Build Pipeline (`pstdr-c`)
- **`pstdr-c build --workspace`**: Runs `check -> convert -> audit` sequentially, emits `// @pstdr <file>:<line>` trace comments into generated JS/TS, syncs `BP/manifest.json`, and verifies zero audit errors.
- Other commands: `pstdr-c check`, `pstdr-c convert`, `pstdr-c audit`, `pstdr-c fmt`, `pstdr-c watch`, `pstdr-c explain <CODE>`, `pstdr-c spec info`, `pstdr-c doctor`, `pstdr-c --version`.

### Editor LSP Setup (`pstdr-lsp`)
- **PackStudio IDE:** Uses the built-in `builtin.pstdr` language server automatically — no manual configuration needed.
- **External Editors (`pstdr-lsp` stdio server):**
  - **Helix** (`~/.config/helix/languages.toml`):
    ```toml
    [language-server.pstdr-lsp]
    command = "pstdr-lsp"

    [[language]]
    name = "pstdr"
    scope = "source.pstdr"
    file-types = ["pstdr"]
    language-servers = ["pstdr-lsp"]
    ```
  - **Neovim** (`nvim-lspconfig` / `vim.lsp.start`):
    ```lua
    vim.api.nvim_create_autocmd("FileType", {
      pattern = "pstdr",
      callback = function()
        vim.lsp.start({ name = "pstdr-lsp", cmd = { "pstdr-lsp" } })
      end,
    })
    ```
  - **VS Code:** Point a generic stdio Language Client extension (or custom `vscode-languageclient` wrapper) to the `pstdr-lsp` binary for `*.pstdr` files.

---

## 8. Compiler Diagnostic Codes (`PSTDR-E001` – `PSTDR-E018`)

| Code | Meaning | Fix |
| :--- | :--- | :--- |
| **`PSTDR-E001`** | Forbidden JS/Lua keyword or operator (`let`, `var`, `function`, `class`, `nil`, `~=`, `..`) in safe mode. | Use `.pstdr` syntax (`const`, `mut`, `func`, `null`, `!=`, `"{a}{b}"`) or top-level `unsafe`. |
| **`PSTDR-E002`** | Reassignment to a `const` variable. | Change declaration from `const` to `mut`. |
| **`PSTDR-E003`** | Reserved `.pstdr` keyword used as identifier (contextual keywords like `title`, `slot`, `cmd`, `min`, `max` are allowed). | Choose a non-reserved identifier name. |
| **`PSTDR-E004`** | `cancel event` used in a non-cancellable context. | Use only in `on right click`, `on break`, `on place`, `on interact`, `on interact_entity`, `on explosion`, or `on chat`. |
| **`PSTDR-E005`** | Variable or context binding not available in current scope. | Declare it or iterate `for p in world.players()` inside timers. |
| **`PSTDR-E006`** | `unsafe` used inside a block or as an expression value. | Declare `unsafe name(...) { ... }` at top level. |
| **`PSTDR-E007`** | `bind_target()` file not found, or `on tick` has no matching `BP/blocks/*.json`. | Create the file or fix the relative path. |
| **`PSTDR-E008`** | Bound JSON `"identifier"` differs from item/block ID in `.pstdr`. | Match `"identifier"` in JSON with the script ID. |
| **`PSTDR-E009`** | Path traversal (`..`) or absolute path in `bind_target`, `mcfunction`, `run_fn`, or UI `icon`. | Use a clean workspace-relative path. |
| **`PSTDR-E010`** | `every` interval `< 50ms` (`1` tick). | Use at least `50ms` (or `1s`, etc.). |
| **`PSTDR-E011`** | Status effect level `< 1` in `apply`. | Use 1-based levels (`1` = Level I). |
| **`PSTDR-E012`** | Unclosed `{` brace. | Add matching closing `}`. |
| **`PSTDR-E013`** | Vanilla effect (`apply`) or sound (`play_sound`) ID not in spec. | Fix spelling against `data/spec/bedrock_api_latest.json`. |
| **`PSTDR-E014`** | Workspace not detected (`BP/` and `pstdr.toml` missing). | Run inside project root or use `convert --from ... to ...`. |
| **`PSTDR-E015`** | Audit Gate failed (bad `min_engine_version`, BP/RP UUID collision, or stale `BP/manifest.json` `@minecraft/server` mismatch). | Run `pstdr-c build --workspace`, use `[1, 26, 60]` / `[1, 21, 50]`, and unique BP/RP UUIDs. |
| **`PSTDR-E016`** | Syntax or API error (unexpected token, helper arity, unknown `world.*`/`system.*`, `< 1.15.0` API, `on cmd` on `1.x`, `runCommandAsync` on `2.x`, non-namespaced `on cmd`/`on script_event` ID). | Fix syntax or update `@addon(api = "...")` to a supported profile. |
| **`PSTDR-E017`** | Invalid texture alias, missing RP image/UI icon, or missing `.mcfunction` for `run_fn`. | Use valid vanilla alias, add RP texture, or define the `.mcfunction`. |
| **`PSTDR-E018`** | Beta-only API (`on chat` / `chatSend` on `2.x` or Beta API in `unsafe`) on stable `api`. | Use `on cmd` (Stable `2.x`), set `api = "2.11.0-beta"` (+ Beta APIs), or target `1.x` LTS. |
