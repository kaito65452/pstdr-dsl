# Quick Start

## Workspace Structure

A `.pstdr` workspace contains:

```text
example-pack/
├── pstdr.toml               # Pack and compiler configuration
├── src/                     # .pstdr source files
├── BP/
│   ├── manifest.json        # Behavior Pack manifest (data + script modules)
│   ├── items/               # Item JSON definitions
│   ├── blocks/              # Block JSON definitions
│   └── scripts/
│       ├── main.js          # Auto-generated entry point
│       └── generated/       # Compiled .js modules
└── build/ts/                # Emitted .ts files (when emit_ts = true)
```

---

## Step 1: Initialize the Workspace

```sh
mkdir example-pack && cd example-pack
pstdr-c init "Example Pack"
```

This generates `pstdr.toml`, `BP/manifest.json` (with `min_engine_version: [1, 26, 60]`, `data` module, `script` module, and `@minecraft/server 2.11.0`), and a starter `src/welcome.pstdr`.

---

## Step 2: Add a Player Join Script

Create `src/hello.pstdr`:

```pstdr
@addon(target = "26.60", api = "2.11.0")

on join of player {
    send "Welcome, {player.name_tag}!" to player
}
```

---

## Step 3: Add a Bound Item

Create `BP/items/wand.json` using a valid vanilla icon alias (`"blaze_rod"`):

```json
{
  "format_version": "1.21.90",
  "minecraft:item": {
    "description": {
      "identifier": "example:wand"
    },
    "components": {
      "minecraft:icon": "blaze_rod"
    }
  }
}
```

Create `src/wand.pstdr`:

```pstdr
@addon(target = "26.60", api = "2.11.0")
bind_target("BP/items/wand.json")

const WAND_ID = "example:wand"

on right click with WAND_ID by player {
    apply speed 2 to player for 5s
    player.actionbar("Boost active")
}
```

---

## Step 4: Check, Convert, and Audit

```sh
pstdr-c check --workspace
pstdr-c convert --workspace
pstdr-c audit --workspace
```

### Expected Output

- `pstdr-c check --workspace` → `Check passed: 0 errors.`
- `pstdr-c convert --workspace` → `Workspace conversion succeeded -> BP/scripts/generated/ & build/ts/`
- `pstdr-c audit --workspace` → `Audit Gate passed: all scripts and manifests are valid.`

### Generated Files

`BP/items/wand.json` is wired automatically for Custom Components V2 (`format_version >= 1.21.90`):

```json
{
  "format_version": "1.21.90",
  "minecraft:item": {
    "components": {
      "minecraft:icon": "blaze_rod",
      "pstdr:wand": {}
    },
    "description": {
      "identifier": "example:wand"
    }
  }
}
```

`BP/scripts/main.js` imports all compiled modules:

```javascript
// Auto-generated entry point by pstdr-c
import "./generated/hello.js";
import "./generated/wand.js";
import "./generated/welcome.js";
```

---

> [← Previous: Installation](./02-Installation.md) | [Next: Language Guide →](./04-Language-Guide.md)
