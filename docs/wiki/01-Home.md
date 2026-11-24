# Welcome to `.pstdr` (PackStudio Language)

**`.pstdr`** is a clean, beginner-friendly language for building **Minecraft Bedrock 26.60** add-ons without wrestling with boilerplate JavaScript, TypeScript, or tricky `manifest.json` rules.

---

## Why `.pstdr`? (30-Second Overview)

Normally, making a simple custom weapon in Minecraft Bedrock means:
- Importing `@minecraft/server` APIs and checking API versions (`1.x` vs `2.x`)
- Writing event subscriptions, null checks, `EquipmentSlot.Mainhand` lookups, and `system.run()` wrappers
- Converting seconds to game ticks (`5s` = `100` ticks) and human effect levels to 0-based amplifiers (`Level 2` = `amplifier: 1`)
- Manually wiring `BP/manifest.json` UUIDs, `data` + `script` modules, `min_engine_version: [1, 26, 60]`, and item JSON custom components

With **`.pstdr`**, you write what you mean in **10 readable lines**:

```pstdr
@addon(target = "26.60", api = "2.11.0")
bind_target("BP/items/frost_scythe.json")

const SCYTHE_ID = "my_pack:frost_scythe"

on hit target with SCYTHE_ID by player {
    apply freezing 1 to target for 4s
    apply slowness 2 to target for 4s
    player.actionbar("§b❄ Frostbite!")
}
```

The compiler (`pstdr-c`) checks your code for mistakes *before* you launch Minecraft, wires your item/block JSONs automatically, and generates clean Bedrock JavaScript + TypeScript.

---

## The 3 Components

| Component | What It Does | Required? |
| :--- | :--- | :--- |
| **`pstdr-c`** | The compiler & CLI tool (`init`, `check`, `convert`, `audit`, `fmt`, `watch`, `explain`). | Yes (installed by default) |
| **`pstdr-lsp`** | Live editor assistant that shows red squiggles and helps format `.pstdr` files as you type. | Optional (enabled by default) |
| **`PackStudio`** | Companion app for PackStudio workflows. | Optional (disabled by default) |

---

## Table of Contents

1. **[Home](./01-Home.md)** — What `.pstdr` is and why it exists
2. **[Installation](./02-Instalasi.md)** — One-line install, component menu, and `PATH` setup
3. **[Your First Add-On](./03-Addon-Pertama.md)** — Build and test `frost_scythe` in 5 minutes
4. **[Code Recipes](./04-Resep-Kode.md)** — Copy-pasteable examples for events, effects, timers, and player state
5. **[Troubleshooting Errors](./05-Mengatasi-Error.md)** — How to read diagnostics and fix `PSTDR-E001` through `PSTDR-E017`
6. **[Editor Setup](./06-Setup-Editor.md)** — Enable live error checking & formatting in Neovim and VS Code

---

> **Next:** [Installation →](./02-Instalasi.md)
