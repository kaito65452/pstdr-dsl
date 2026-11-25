# pstdr-dsl

A beginner-friendly DSL and toolchain for building **Minecraft Bedrock 26.60** add-ons without boilerplate JavaScript or manual manifest wiring.

> ### For AI / LLM Code Generation
>
> **Read [`llms.txt`](./llms.txt) (compact spec) and [`docs/LANGUAGE_REFERENCE.md`](./docs/LANGUAGE_REFERENCE.md) (full in-repo spec) BEFORE writing `.pstdr` code.** Do not infer language syntax from a single example.
>
> **Strict Syntax & Anti-Hallucination Rules:**
> - **No JS/Lua keywords in safe mode (`PSTDR-E001`):** Use `const`, `mut`, `func`, `null`, `!=`, and `"{expr}"` — never `let`, `var`, `function`, `class`, `new`, `~=`, `..`, or single/backtick quotes outside top-level `unsafe`.
> - **Use per-player `state key[player] = default`:** Never store per-player timers, cooldowns, or scores in a single global `mut` variable.
> - **No invented Bedrock/DSL APIs:** There is NO `world.get_time()` (use `every`/`after` or `system.currentTick`), NO 2-arg `p.title(a, b)` (call `p.title(a)` then `p.subtitle(b)`), and NO `runCommandAsync`.
> - **Use Wave 3A commands & bridges:** Prefer `command on player "..."`, `run_fn on player "folder/fn"`, inline `mcfunction "folder/fn" { ... }`, and `on script_event "ns:id"` over raw `run_command(...)`.
> - **Stable vs. Beta & Assets:** `on chat` requires `api = "2.11.0-beta"` (`PSTDR-E018`); every custom item needs a `BP/items/*.json` with a valid vanilla `minecraft:icon` alias or registered `RP/` texture (`PSTDR-E017`).

## Quick Install

```sh
curl -fsSL https://raw.githubusercontent.com/kaito65452/pstdr-dsl/main/install.sh | sh
```

## Documentation

- **[`llms.txt`](./llms.txt)** — Compact, high-signal language specification for LLMs and coding agents.
- **[`docs/LANGUAGE_REFERENCE.md`](./docs/LANGUAGE_REFERENCE.md)** — Complete in-repo language, event, action, and error-code reference (`PSTDR-E001`–`PSTDR-E018`).
- **[`docs/wiki/01-Overview.md`](./docs/wiki/01-Overview.md)** — Step-by-step tutorials and installation guides.

## Contrasting Patterns

### 1. Per-Player `state[player]` Cooldown Mechanic

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

### 2. Wave 3A `mcfunction` + `on script_event` Bridge

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

## Showcase / Real-World Example

> **Note:** **Zombie Rush** is only *one* wave-defense minigame example — it is **not** a universal template for all add-ons or minigames. For custom items, per-player mechanics, block ticks, or command/`mcfunction` bridges, design from **[`llms.txt`](./llms.txt)** and **[`docs/LANGUAGE_REFERENCE.md`](./docs/LANGUAGE_REFERENCE.md)** instead of reskinning `Zombie Rush`.

See **[Zombie Rush](./docs/zombie-rush-example/README.md)** — a 106-line minigame built in a blind test by Claude, using `0 unsafe` blocks, and verified in-game on Minecraft Bedrock 26.60.
