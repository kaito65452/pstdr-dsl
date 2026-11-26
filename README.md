# pstdr-dsl

A beginner-friendly DSL and toolchain for building **Minecraft Bedrock** add-ons targeting both **Modern 2.x** (Bedrock `26.60`, `@minecraft/server 2.11.0`) and **Legacy 1.x LTS** (Bedrock `1.21.50+`, `@minecraft/server 1.15.0`–`1.19.x`) without boilerplate JavaScript or manual manifest wiring.

> ### For AI / LLM Code Generation
>
> **Read [`llms.txt`](./llms.txt) (compact spec) and [`docs/LANGUAGE_REFERENCE.md`](./docs/LANGUAGE_REFERENCE.md) (full in-repo spec) BEFORE writing `.pstdr` code.** Do not infer language syntax from a single example.
>
> **Strict Syntax & Anti-Hallucination Rules:**
> - **No JS/Lua keywords in safe mode (`PSTDR-E001`):** Use `const`, `table`, `mut`, `func`, `import { ... } from "..."`, `export`, `null`, `!=`, and `"{expr}"` — never `let`, `var`, `function`, `class`, `new`, `~=`, `..`, or single/backtick quotes outside top-level `unsafe`.
> - **Exact duration units:** `ms`, `s`, `m` ONLY (there is NO `t` tick suffix; use `50ms` for 1 tick).
> - **Use per-player `state key[player] = default`:** Never store per-player timers, cooldowns, or scores in a single global `mut` variable.
> - **No invented Bedrock/DSL APIs:** There is NO `world.get_time()` (use `every`/`after` or `system.current_tick`), NO 2-arg `p.title(a, b)` (call `p.title(a)` then `p.subtitle(b)`), and NO `runCommandAsync` on `2.x` (use `command on player "..."` or `run_fn`).
> - **Wave 4 RPG Modules & Primitives:** Supports multi-file `import`/`export`, `table` configs, wildcard/generic triggers with `when [not] sneaking`, `player.armor.*`/`offhand`, `heal`, `damage ... cause ... source ...`, `apply effect(...)`, `play custom_sound`, `raycast entities`, `entities in cone(...)`, `launch`, `dash`, dynamic `after (<expr>)`, and `timeline { ... }`.
> - **Commands, UI & Profiles:** Use **`on cmd`** for Stable `2.x` slash commands (`on chat` is `2.x` Beta-only / `1.x` Stable), `ui action/message/modal` (auto-injects `@minecraft/server-ui` only when used), and `pstdr-c build --workspace` to check, compile, and audit in one command.

## Quick Install & Build

```sh
curl -fsSL https://raw.githubusercontent.com/kaito65452/pstdr-dsl/main/install.sh | sh

# Validate, compile, and audit a workspace in one step:
pstdr-c build --workspace
```

## Documentation

- **[`llms.txt`](./llms.txt)** — Compact, high-signal language specification for LLMs and coding agents.
- **[`docs/LANGUAGE_REFERENCE.md`](./docs/LANGUAGE_REFERENCE.md)** — Complete in-repo language, event, UI, scoreboard, `ApiProfile`, and error-code reference (`PSTDR-E001`–`PSTDR-E018`).
- **[`docs/wiki/01-Overview.md`](./docs/wiki/01-Overview.md)** — Step-by-step tutorials and installation guides.

## Contrasting Patterns

### 1. Stable `2.11.0` Custom Command (`on cmd`) + Native UI (`ui action`) + Scoreboard

```pstdr
@addon(target = "26.60", api = "2.11.0")

on cmd "my_pack:kit" description "Open kit menu" permission any by player {
    ui action "Starter Kits" body "Choose your loadout:" to player {
        button "Warrior" icon "textures/items/iron_sword" {
            player.give_item("minecraft:iron_sword", 1, "§bStarter Blade")
            score["kits"][player] += 1
            scoreboard show "kits" title "Kits Claimed" slot sidebar
            player.play_sound("random.levelup")
        }
        button "Runner" icon "textures/items/feather" {
            apply speed 2 to player for 10s
            player.play_sound("random.orb")
        }
    }
}
```

### 2. Per-Player `state[player]` Cooldown Mechanic

```pstdr
@addon(target = "26.60", api = "2.11.0")

state dash_cd[player] = 0

on right click with "minecraft:feather" by player {
    if dash_cd[player] > 0 {
        player.actionbar("Cooldown: {dash_cd[player]}s")
        return
    }
    dash_cd[player] = 5
    apply speed 2 to player for 2s
    player.play_sound("random.orb")
}

every 1s {
    for p in world.players() {
        if dash_cd[p] > 0 {
            dash_cd[p] -= 1
        }
    }
}
```

### 3. Wave 3A `mcfunction` + `on script_event` Bridge

```pstdr
@addon(target = "26.60", api = "2.11.0")

mcfunction "kit/reset" {
    effect @s clear
    clear @s
}

on script_event "my_pack:reset" {
    if source != null {
        run_fn on source "kit/reset"
        command on source "gamemode adventure @s"
        send "Kit reset complete." to source
    }
}
```

## Editor LSP Support

- **PackStudio IDE:** Powered by the built-in `builtin.pstdr` language server automatically.
- **External Editors (`pstdr-lsp`):** The workspace builds a standard stdio LSP binary (`pstdr-lsp`) with diagnostics, hover, completions, formatting, and quick fixes. See [`docs/LANGUAGE_REFERENCE.md#editor-lsp-setup-pstdr-lsp`](./docs/LANGUAGE_REFERENCE.md#editor-lsp-setup-pstdr-lsp) for Helix, Neovim, and VS Code configs.

## Showcase / Real-World Example

> **Note:** **Zombie Rush** is only *one* wave-defense minigame example — it is **not** a universal template for all add-ons or minigames. For custom commands, UI menus, scoreboards, per-player mechanics, or `mcfunction` bridges, design from **[`llms.txt`](./llms.txt)** and **[`docs/LANGUAGE_REFERENCE.md`](./docs/LANGUAGE_REFERENCE.md)** instead of reskinning `Zombie Rush`.

See **[Zombie Rush](./docs/zombie-rush-example/README.md)** — a 106-line minigame built in a blind test by Claude, using `0 unsafe` blocks, and verified in-game on Minecraft Bedrock 26.60.
