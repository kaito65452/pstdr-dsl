# Zombie Rush — Real-World Minigame Example

A complete 60-second zombie-killing minigame for **Minecraft Bedrock 26.60** (`@minecraft/server` `2.11.0`) written in a single **106-line** [zombie_rush.pstdr](./zombie_rush.pstdr) file—behavior pack only, zero custom textures or Resource Pack required, and **0 `unsafe` blocks**.

> **Verification Status:** Authored independently in a **blind test by Claude** using only this repository and [`docs/wiki/`](../wiki/01-Overview.md). It passed `pstdr-c check`, `pstdr-c convert`, and `pstdr-c audit` on the first try and was **tested in-game on Minecraft Bedrock 26.60 (`@minecraft/server` 2.11.0)**, where it ran smoothly end to end.

---

## How It Plays

1. **First Join:** Each player receives a welcome message and is automatically granted the **Zombie Rush Starter** item (`arena:starter`, using the vanilla `blaze_rod` icon) once, tracked via per-player persistent state (`got_item[player]`).
2. **Start / Stop a Round:**
   - **Right-click** with `arena:starter` to start a 60-second round. All online players receive a title/subtitle banner, 10 seconds of **Resistance II**, and a level-up sound cue.
   - **Sneak + right-click** with `arena:starter` while a round is active to stop the round early.
3. **Wave Spawning:** Every `3s`, two zombies spawn around each player (at `x ± 6, z ± 6` in the player's current dimension) and are tagged with `arena_mob`.
4. **Live Action Bar HUD:** Every `1s`, each player's action bar displays remaining time, current round kills, and persistent personal best (`Time: <t>s   Kills: <k>   Best: <b>`).
5. **Scoring & Round End:** Killing a tagged zombie increments `score[killer]` and plays `random.orb`. When the timer hits `0` (or the round is stopped early), the winner is broadcast to the world, new personal bests are saved to `best[p]`, and all leftover `arena_mob` zombies in the overworld are cleaned up with `z.kill()`.

### Configurable Top-Level Constants

You can tune the minigame directly at the top of [zombie_rush.pstdr](./zombie_rush.pstdr):

| Constant | Default Value | Purpose |
| :--- | :--- | :--- |
| `STARTER_ID` | `"arena:starter"` | Custom item identifier that starts and stops rounds. |
| `ROUND_SECONDS` | `60` | Round duration in seconds. |
| `MOB_TAG` | `"arena_mob"` | Entity tag used to track spawned wave zombies for scoring and cleanup. |

---

## What This Example Demonstrates in `.pstdr`

- **Single-File Identifier-Driven Partitioning:** Even though all logic lives in one file (`zombie_rush.pstdr`), when compiled inside a workspace containing `BP/items/starter.json` (`arena:starter`), `pstdr-c convert --workspace` automatically partitions it into:
  - `BP/scripts/generated/_shared/zombie_rush.shared.js` — shared constants, helper functions (`start_round`, `end_round`), `state` accessors, and mutable globals
  - `BP/scripts/generated/items/starter.js` — custom item component (`onUse`) for `arena:starter`
  - `BP/scripts/generated/world/zombie_rush.js` — global events (`on join`, `on die`) and timers (`every 1s`, `every 3s`)
  - `BP/scripts/generated/_registry.js` — centralized `system.beforeEvents.startup` custom component registration
- **Cross-Module `mut` & Persistent `state`:** Top-level mutable variables (`active`, `time_left`) are automatically wrapped in `export const __pstdr_mut = { ... }` so split ES modules can mutate them safely, while per-player `state` (`score`, `best`, `got_item`) compiles to Bedrock Dynamic Properties (`pstdr:score`, `pstdr:best`, `pstdr:got_item`).
- **Automatic Wiring & Zero Boilerplate:** `BP/manifest.json` (`min_engine_version: [1, 26, 60]`, `data` + `script` modules, `@minecraft/server` `2.11.0`), Custom Components V2 item JSON wiring (`"pstdr:starter": {}`), and `BP/scripts/main.js` imports are all generated automatically.
- **Pure Safe-Mode DSL:** Built with **0 `unsafe` blocks** using built-in events, timers, actions, predicates (`player is sneaking`), and `snake_case` Bedrock helpers.

---

## How to Build & Run

Because this directory contains the standalone source file [zombie_rush.pstdr](./zombie_rush.pstdr), you can build a complete playable Behavior Pack workspace in a few steps:

### 1. Initialize a Workspace and Copy the Source

```sh
mkdir zombie-rush && cd zombie-rush
pstdr-c init "Zombie Rush"
rm -f src/welcome.pstdr
cp /path/to/pstdr-dsl/docs/zombie-rush-example/zombie_rush.pstdr src/zombie_rush.pstdr
```

### 2. Add the Custom Starter Item JSON (`BP/items/starter.json`)

Create `BP/items/starter.json` using the vanilla `"blaze_rod"` icon (no Resource Pack required):

```json
{
  "format_version": "1.21.90",
  "minecraft:item": {
    "description": {
      "identifier": "arena:starter"
    },
    "components": {
      "minecraft:icon": "blaze_rod",
      "minecraft:max_stack_size": 1,
      "minecraft:display_name": {
        "value": "Zombie Rush Starter"
      }
    }
  }
}
```

### 3. Check, Convert, and Audit

```sh
pstdr-c check --workspace
pstdr-c convert --workspace
pstdr-c audit --workspace
```

### 4. Install in Minecraft Bedrock 26.60

Copy the generated `BP/` directory into your Minecraft Bedrock `development_behavior_packs/` folder (or package `BP/` as a `.mcpack`), enable the Behavior Pack on a world in **Minecraft Bedrock 26.60**, and join the world to receive your Starter item.

---

## Blind-Test Report (Claude's Direct Feedback)

The following is a direct summary of Claude's feedback after building Zombie Rush from scratch using only the repository and documentation:

### Ease of Use (Very Good, with Caveats)
- The full workflow (`init`, `check`, `convert`, `audit`) succeeded on the first try.
- Boilerplate was handled automatically: manifest generation, item component registration, `main.js` imports, and automatic splitting of a single `.pstdr` file into item, world, and shared modules.
- Both per-player `state` (Dynamic Properties) and cross-module top-level `mut` held up cleanly in the generated JavaScript.
- The syntax was easy to guess (`snake_case` helpers, `send ... to`, `apply ... for 5s`), and the wiki documentation was sufficient to write ~100 lines without trial and error.

### Key Takeaways & Limitations Noted
- `pstdr-c check` and `pstdr-c audit` validate static concerns (syntax, variable scope, vanilla effect/sound IDs, manifest and asset checks) rather than runtime gameplay balance or mob spawn placement.
- The current built-in event catalog is concise (11 events); areas such as player leave, chat, scoreboards, and UI forms are not covered natively yet.
- The toolchain targets **Minecraft Bedrock 26.60** with `@minecraft/server` `2.11.0` (`0.1.0-alpha.1`), so a matching Bedrock version is required.

### Zero `unsafe` Usage
- **No `unsafe` blocks were needed** to build the entire minigame.
- When an escape hatch *is* required (for example, constructing `ItemStack`, passing particle `MolangVariableMap`s, or calling `@minecraft/server-ui`), `unsafe` is intentionally restricted to top-level declarations (inline or expression usage triggers `PSTDR-E006`), while safe mode blocks `new`, `class`, `let`, `import`, and single-quoted/backtick strings. Because raw `unsafe` code bypasses static checks and automatic `system.run` wrapping, it is best kept as small, isolated top-level helper functions.

---

> [← Back to Overview](../wiki/01-Overview.md) | [Quick Start →](../wiki/03-Quick-Start.md)
