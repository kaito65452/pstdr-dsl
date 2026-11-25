# pstdr-dsl Language Reference

> Authoritative in-repo specification for **pstdr-dsl**, derived directly from the compiler source (`crates/pstdr-lang/src/`).
> Target platform: **Minecraft Bedrock 26.60** (`@minecraft/server` `2.11.0` stable / `2.11.0-beta`).
> For a compact LLM/agent prompt sheet, see [llms.txt](../llms.txt).

---

## 1. File Directives

```pstdr
@addon(target = "26.60", api = "2.11.0")
bind_target("BP/items/frost_blade.json")
```

- **`@addon(target = "<ver>", api = "<ver>")`**
  - Optional; at most **one** per file (duplicate `@addon` fails with `PSTDR-E016`).
  - Defaults to `target = "26.60"`, `api = "2.11.0"` (loaded from `data/spec/bedrock_api_latest.json`).
  - Only `target` and `api` attributes are supported, and values must be double-quoted string literals.
  - **Stable vs. Beta APIs (`PSTDR-E018`):** Use `api = "2.11.0"` for all stable add-ons. To use Beta-only features such as `on chat` (`world.beforeEvents.chatSend`), you must set `api = "2.11.0-beta"` and enable the **Beta APIs** experiment in Minecraft world settings.
- **`bind_target("<relative_path>")`**
  - Optional; multiple directives allowed.
  - Relative path from workspace root to a `BP/items/*.json` or `BP/blocks/*.json` file. No `..` or absolute paths (`PSTDR-E009`).
  - Compiler checks the file exists (`PSTDR-E007`), wires custom components, and verifies that the JSON `"identifier"` matches the item/block ID used in your script (`PSTDR-E008`).

---

## 2. Top-Level Declarations

Every top-level item must be one of the following:

| Construct | Syntax | Description |
| :--- | :--- | :--- |
| Constant | `const <name> = <expr>` | Immutable binding; reassignment triggers `PSTDR-E002`. |
| Mutable Variable | `mut <name> = <expr>` | World/module-scoped mutable binding. Supports `=`, `+=`, `-=`, `*=`, `/=`. |
| Persistent State | `state <name>[<owner>] = <default>` | Per-entity Dynamic Property (`pstdr:<name>`). |
| Function | `func <name>(<p1>, <p2>) { ... }` | Validated `.pstdr` function. |
| Inline `.mcfunction` *(Wave 3A)* | `mcfunction "<path>" { ... }` | Emits raw Bedrock commands to `BP/functions/<path>.mcfunction`. No JS output. |
| Repeating Timer | `every <duration> { ... }` | Compiles to `system.runInterval(..., ticks)`. Minimum `50ms` (`PSTDR-E010`). |
| One-Shot Timer | `after <duration> { ... }` | Compiles to `system.runTimeout(..., ticks)`. |
| Event Handler | `on <event> { ... }` | Subscribes to Bedrock `world`/`system` events or custom component callbacks. |
| Unsafe Function | `unsafe <name>(<p1>, ...) { ... }` | Top-level raw JS/TS escape hatch. |
| Unsafe Variable | `unsafe const\|mut\|var\|let <name> = ...;` | Single-line raw JS/TS declaration at top level (`mut` emits `let`). |

### Multiplayer State Rule

Always use `state <key>[player] = <default>` for per-player cooldowns, scores, timers, and flags. Never store per-player values in a single global `mut` variable.

```pstdr
state dash_cd[player] = 0

every 1s {
    for p in world.players() {
        if dash_cd[p] > 0 {
            dash_cd[p] -= 1
        }
    }
}
```

### Inline `mcfunction` Blocks (Wave 3A)

```pstdr
mcfunction "arena/reset" {
    // Line comments are stripped when emitted
    effect @s clear
    tp @s 0 64 0
}
```

- Path must be a non-empty relative path without `..` or leading slashes (`PSTDR-E009`).
- Emitted directly to `BP/functions/<path>.mcfunction`.
- Recognized automatically by `run_fn` validation (`PSTDR-E017`).

---

## 3. Event Handlers (Wave 1–3A)

| Event | Exact Syntax | Scope Variables | Underlying Bedrock Hook |
| :--- | :--- | :--- | :--- |
| **Right Click** | `on right click with <item> by <actor> { ... }` | `<actor>` | `afterEvents.itemUse` / `beforeEvents.itemUse` (or item component `onUse`) |
| **Hit Entity** | `on hit <target> with <item> by <actor> { ... }` | `<target>`, `<actor>` | `afterEvents.entityHitEntity` (or item component `onHitEntity`) |
| **Consume Item** | `on consume <item> by <actor> { ... }` | `<actor>` | `afterEvents.itemCompleteUse` (or item component `onConsume`) |
| **Break Block** | `on break of <block_id> by <actor> { ... }` | `<actor>`, `block` | `afterEvents.playerBreakBlock` / `beforeEvents.playerBreakBlock` (or `onPlayerBreak`) |
| **Block Interact** | `on interact of <block_id> by <actor> { ... }` | `<actor>`, `block` | Block component `onPlayerInteract` or `playerInteractWithBlock` |
| **Block Tick** | `on tick of <block_id> { ... }` | `block`, `dimension` | Custom block component `onTick` (requires matching `BP/blocks/*.json`, `PSTDR-E007`) |
| **Player Join** | `on join of <actor> { ... }` | `<actor>` | `world.afterEvents.playerSpawn` (filtered to `ev.initialSpawn`) |
| **Player Leave** | `on leave of <actor> { ... }` | `<actor>` | `world.beforeEvents.playerLeave` (auto `system.run` for mutations) |
| **Chat Send** *(Beta)* | `on chat <msg> by <actor> { ... }` | `<msg>`, `<actor>` | `world.beforeEvents.chatSend` (**requires `api = "2.11.0-beta"`**, `PSTDR-E018`) |
| **Entity Hurt** | `on hurt <target> [by <attacker>] { ... }` | `<target>`, `[<attacker>]` | `world.afterEvents.entityHurt` |
| **Entity Die** | `on die of <target> [by <killer>] { ... }` | `<target>`, `[<killer>]` | `world.afterEvents.entityDie` |
| **Projectile Hit** | `on projectile hit <target> [with <proj_id>] [by <shooter>] { ... }` | `<target>`, `[<shooter>]`, `projectile` | `world.afterEvents.projectileHitEntity` |
| **Entity Spawn** | `on spawn of <entity> [type <type_id>] { ... }` | `<entity>` | `world.afterEvents.entitySpawn` |
| **Script Event** *(Wave 3A)* | `on script_event "<namespace>:<id>" { ... }` | `event`, `source`, `source_block` | `system.afterEvents.scriptEventReceive` (filtered by namespace & ID) |
| **Entity Trigger** *(Wave 3A)* | `on entity_trigger "<event_id>" [for "<entity_type>"] { ... }` | `entity`, `event_id` | `world.afterEvents.dataDrivenEntityTrigger` |

### `beforeEvents` Read-Only Safety

`on leave` and `on chat` always use `world.beforeEvents`; `on right click`, `on break`, and `on interact` automatically switch to `world.beforeEvents` when they contain `cancel event`. In `beforeEvents`, `ev.cancel = true` and read-only bindings execute synchronously, while world-mutating actions (`damage`, `ignite`, `apply`, `send`, `spawn particle`, `command`, `run_fn`, `state` writes, and expression statements) are automatically wrapped in `system.run(() => { ... })`.

### `on script_event` Validation

The `<event_id>` in `on script_event` must be a static string literal or `const` containing a namespace colon (`"<namespace>:<id>"`) with no whitespace (`PSTDR-E016`). Inside the handler:
- `event`: raw `ScriptEventCommandMessageAfterEvent` (`event.id`, `event.message`, etc.)
- `source`: `event.sourceEntity`
- `source_block`: `event.sourceBlock`

---

## 4. Statements & Built-in Actions

Semicolons (`;`) are optional at the end of statements.

### Variables & Assignment
- `const <name> = <expr>`
- `mut <name> = <expr>`
- `<target> = <expr>` | `+=` | `-=` | `*=` | `/=` (target may be a `mut` identifier, `state[owner]`, array/object index, or object property).

### Control Flow & Timers
- `if <cond> { ... } else if <cond> { ... } else { ... }`
- `for <var> in <iterable> { ... }` (compiles to `for (const <var> of <iterable>)`)
- `while <cond> { ... }`
- `return` or `return <expr>` (expression must start on the same line as `return`)
- `after <duration> { ... }` (nested or top-level)
- `every <duration> { ... }` (nested or top-level; minimum `50ms`, `PSTDR-E010`)

### Action Statements

| Action | Syntax | Compiled Output / Notes |
| :--- | :--- | :--- |
| **Cancel Event** | `cancel event` | `ev.cancel = true;`. Only valid in `on right click`, `on break`, `on interact`, `on chat` (`PSTDR-E004`). |
| **Send Message** | `send <msg> to <target>` | `<target>.sendMessage(<msg>);` (`<target>` can be a player or `world`). |
| **Apply Effect** | `apply <effect> <level> to <target> for <duration>` | `<target>.addEffect("<effect>", <ticks>, { amplifier: <level> - 1 });`. `<level>` is 1-based (`>= 1`, `PSTDR-E011`). `<effect>` checked against spec (`PSTDR-E013`). |
| **Damage** | `damage <target> by <amount>` | `<target>.applyDamage(<amount>);` |
| **Ignite** | `ignite <target> for <duration>` | `<target>.setOnFire(<seconds>, true);` |
| **Spawn Particle** | `spawn particle <id> at <entity_or_loc> [in <dim>]` | `<dim>.spawnParticle(<id>, <loc>);`. Infers dimension from entity or `.location`/`.head_location`; defaults to `"overworld"`. |
| **Run Command** *(Wave 3A)* | `command [on <target>] <cmd_expr>` | `<target>.runCommand(<cmd>);` (or `world.getDimension("overworld").runCommand(<cmd>);` if `on <target>` is omitted). |
| **Run `.mcfunction`** *(Wave 3A)* | `run_fn [on <target>] <path_expr>` | Runs `function <path>` on `<target>` (or overworld). Static paths are validated against `BP/functions/<path>.mcfunction` and inline `mcfunction` blocks (`PSTDR-E017`, `PSTDR-E009`). |

---

## 5. Literals, Operators & Predicates

### Literals & Units
- **Numbers:** `42`, `1.5`
- **Booleans & Null:** `true`, `false`, `null`
- **Durations:** `50ms` (`1` tick), `3s` (`60` ticks), `2m` (`2400` ticks). In expressions, emits tick count.
- **Distances:** `5b`, `5blocks` (emits numeric block count).
- **Strings:** Double-quoted `"..."` only, with `{expr}` interpolation (`"HP: {target.hp}"`). Supported escapes: `\n`, `\r`, `\t`, `\\`, `\"`, `\{`, `\}`. Single quotes and backticks are rejected outside `unsafe` (`PSTDR-E016`).
- **Arrays:** `[1, "a", true]`
- **Objects:** `{ key = val, "str_key": val }` (both `=` and `:` are accepted).

### Operators (Lowest to Highest Precedence)
1. Ternary: `cond ? then_expr : else_expr`
2. Logical OR: `or`, `||`
3. Logical AND: `and`, `&&`
4. Logical NOT (keyword): `not <expr>`
5. Comparison & Membership: `==`, `!=`, `<`, `<=`, `>`, `>=`, `is [not] <predicate>`, `in`, `within` (Euclidean distance `<= 5` blocks)
   - `==` / `!=` with `null` emit JS `==` / `!=`; otherwise emit strict `===` / `!==`.
6. Nullish Coalescing: `??`
7. Additive: `+`, `-`
8. Multiplicative: `*`, `/`, `%`
9. Unary: `-`, `!`
10. Postfix: `.field`, `?.field`, `[index]`, `(args...)` (indexing/call must be on the same line as the callee).

### `is` / `is not` Predicates
`sneaking`, `sprinting`, `swimming`, `flying`, `sleeping`, `on_ground` (or `on ground`), `in_water` (or `in water`), `holding <item_id>`.

---

## 6. Built-in Globals & `snake_case` API Mappings

### Built-in Globals
`world`, `system`, `console`, `Math`, `JSON`, `String`, `Number`, `Boolean`, `Array`, `Object`, `Date`, `parseInt`, `parseFloat`, `isNaN`.

### Global `world` Methods
| `.pstdr` | Emitted Bedrock API |
| :--- | :--- |
| `world.players()` | `world.getAllPlayers()` |
| `world.send_message(msg)` | `world.sendMessage(msg)` |
| `world.dimension(id)` | `world.getDimension(id)` |
| `world.play_sound(id, loc)` | `world.playSound(id, loc)` |

### Entity & Player Properties
| `.pstdr` | Emitted Bedrock API |
| :--- | :--- |
| `name_tag` | `nameTag` |
| `type_id` | `typeId` |
| `is_sneaking`, `is_sprinting`, `is_swimming`, `is_flying`, `is_sleeping`, `is_on_ground`, `is_in_water`, `is_climbing`, `is_falling`, `is_valid` | camelCase equivalents (`isSneaking`, ..., `isValid`) |
| `selected_slot` | `selectedSlotIndex` |
| `head_location` | `getHeadLocation()` |
| `view_direction` | `getViewDirection()` |
| `hp` | `getComponent("minecraft:health")?.currentValue` |
| `max_hp` | `getComponent("minecraft:health")?.effectiveMax` |
| `holding` / `mainhand` | `getComponent("minecraft:equippable")?.getEquipment(EquipmentSlot.Mainhand)` |

**ItemStack properties** (chained after `.mainhand` / `.holding`): `id` / `type_id` → `typeId`, `name_tag` → `nameTag`.

### Entity, Player & Dimension Methods
| `.pstdr` | Emitted Bedrock API |
| :--- | :--- |
| `actionbar(msg)` | `onScreenDisplay.setActionBar(msg)` |
| `title(msg)` | `onScreenDisplay.setTitle(msg)` *(1 argument only)* |
| `subtitle(msg)` | `onScreenDisplay.updateSubtitle(msg)` |
| `send_message(msg)` | `sendMessage(msg)` |
| `play_sound(id)` | `playSound(id)` *(vanilla sound ID validated by `PSTDR-E013`)* |
| `add_tag(t)` / `remove_tag(t)` / `has_tag(t)` / `get_tags()` | `addTag` / `removeTag` / `hasTag` / `getTags` |
| `apply_damage(n)` | `applyDamage(n)` |
| `apply_impulse(vec)` | `applyImpulse(vec)` |
| `apply_knockback(x, z, h, v)` | `applyKnockback(x, z, h, v)` |
| `run_command(cmd)` | `runCommand(cmd)` *(prefer `command on <target> "..."`)* |
| `add_effect(id, dur, opts)` / `remove_effect(id)` | `addEffect` / `removeEffect` |
| `set_on_fire(secs, ext)` / `extinguish_fire()` | `setOnFire` / `extinguishFire` |
| `spawn_particle(id, loc)` / `spawn_entity(id, loc)` | `spawnParticle` / `spawnEntity` |
| `spawn_item(item_id, loc_or_entity, amount?)` | `__pstdr_spawn_item(...)` *(auto-imports `ItemStack`)* |
| `give_item(item_id, amount?, name_tag?)` | `__pstdr_give_item(...)` *(auto-imports `ItemStack`)* |
| `get_score(obj)` | `__pstdr_get_score(target, obj)` *(null-safe, defaults to `0`)* |
| `set_score(obj, val)` / `add_score(obj, delta)` | `__pstdr_set_score` / `__pstdr_add_score` *(auto-creates objective)* |
| `holding(item_id)` | Equippable Mainhand `typeId === item_id` check |
| `get_entities(opts)` / `get_players(opts)` | `getEntities` / `getPlayers` |
| `teleport(loc)` / `kill()` | `teleport(loc)` / `kill()` |

---

## 7. Keywords, Forbidden Syntax & Escape Hatches

- **Reserved keywords (`PSTDR-E003` if used as identifier):**
  `const`, `mut`, `state`, `func`, `unsafe`, `bind_target`, `on`, `if`, `else`, `for`, `while`, `return`, `and`, `or`, `not`, `is`, `in`, `within`, `null`, `true`, `false`, `cancel`, `damage`, `ignite`, `apply`, `send`, `command`, `run_fn`, `mcfunction`, `after`, `every`, `with`, `by`, `to`, `of`, `at`.
- **Forbidden keywords & operators in safe mode (`PSTDR-E001`):**
  `class`, `extends`, `super`, `new`, `this`, `constructor`, `var`, `let`, `function`, `nil`, `then`, `end`, `import`, `export`, `require`, `eval`, `~=`, `..`.
- **`unsafe` escape hatch:**
  - Must be declared at top level: `unsafe helper(p) { ... }` or `unsafe const X = ...;`. Cannot be used inside blocks or as an expression (`PSTDR-E006`).
  - Bypasses `.pstdr` keyword and vanilla ID checks, **except** Beta API checks (`PSTDR-E018` still guards Beta-only events/APIs when targeting a stable `@minecraft/server` version).

---

## 8. Assets, Manifests & Custom Items

- Custom item IDs (e.g. `"my_pack:wand"`) require a matching `BP/items/<name>.json` file.
- `minecraft:icon` in `BP/items/*.json` (and `minecraft:material_instances` in `BP/blocks/*.json`) is checked by the Asset Guard (`PSTDR-E017`):
  - Must NOT contain `/`, `\`, `:`, or file extensions (`.png`, `.tga`, `.jpg`).
  - Either use a valid vanilla texture alias (e.g. `"blaze_rod"`, `"iron_sword"`) without needing `RP/`, or register a custom alias in `RP/textures/item_texture.json` / `terrain_texture.json` pointing to an existing `.png`/`.tga` file in `RP/`.
- `BP/manifest.json` `min_engine_version` MUST start with `1` (e.g. `[1, 26, 60]` for Bedrock 26.60 — never `[26, 60, 0]`, `PSTDR-E015`).

---

## 9. Compiler Diagnostic Codes (`PSTDR-E001` – `PSTDR-E018`)

| Code | Meaning | Fix |
| :--- | :--- | :--- |
| **`PSTDR-E001`** | Forbidden keyword/operator in safe mode (`let`, `var`, `function`, `class`, `nil`, `~=`, `..`, etc.). | Use `.pstdr` syntax (`const`, `mut`, `func`, `null`, `!=`, `"{a}{b}"`) or top-level `unsafe`. |
| **`PSTDR-E002`** | Reassignment to a `const` variable. | Change declaration from `const` to `mut`. |
| **`PSTDR-E003`** | Reserved `.pstdr` keyword used as an identifier. | Choose a non-reserved identifier name. |
| **`PSTDR-E004`** | `cancel event` used in a non-cancellable context. | Use only in `on right click`, `on break`, `on interact`, or `on chat`. |
| **`PSTDR-E005`** | Variable or context binding not available in current scope. | Declare it or iterate `for p in world.players()` inside timers. |
| **`PSTDR-E006`** | `unsafe` used inside a block or as an expression value. | Declare `unsafe name(...) { ... }` at top level. |
| **`PSTDR-E007`** | `bind_target()` file not found, or `on tick` has no matching `BP/blocks/*.json`. | Create the file or fix the relative path. |
| **`PSTDR-E008`** | Bound JSON `"identifier"` differs from item/block ID in `.pstdr`. | Match `"identifier"` in JSON with the script ID. |
| **`PSTDR-E009`** | Path traversal (`..`) or absolute path in `bind_target`, `mcfunction`, or `run_fn`. | Use a clean workspace-relative path. |
| **`PSTDR-E010`** | `every` interval `< 50ms` (`1` tick). | Use at least `50ms` (or `1s`, etc.). |
| **`PSTDR-E011`** | Status effect level `< 1` in `apply`. | Use 1-based levels (`1` = Level I). |
| **`PSTDR-E012`** | Unclosed `{` brace. | Add matching closing `}`. |
| **`PSTDR-E013`** | Vanilla effect (`apply`) or sound (`play_sound`) ID not in spec. | Fix spelling against `data/spec/bedrock_api_latest.json`. |
| **`PSTDR-E014`** | Workspace not detected (`BP/` and `pstdr.toml` missing). | Run inside project root or use `convert --from ... to ...`. |
| **`PSTDR-E015`** | Audit Gate failed (bad `min_engine_version` or BP/RP UUID collision). | Use `[1, 26, 60]` and unique BP/RP UUIDs. |
| **`PSTDR-E016`** | General syntax error (unexpected token, unclosed quote, invalid `script_event` ID). | Fix syntax or use `"namespace:id"` for `script_event`. |
| **`PSTDR-E017`** | Invalid texture alias, missing RP image/registry entry, or missing `.mcfunction` for `run_fn`. | Use valid vanilla alias, add RP texture, or define the `.mcfunction`. |
| **`PSTDR-E018`** | Beta-only API (`on chat` / `chatSend` / Beta API in `unsafe`) on stable `api`. | Set `api = "2.11.0-beta"` (+ enable Beta APIs) or use stable events. |
