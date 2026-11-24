# Your First Add-On in 5 Minutes

In this guide, you will create a custom item called **Frost Scythe** (`my_pack:frost_scythe`) for **Minecraft Bedrock 26.60** that freezes and slows enemies on hit.

---

## Step 1: Initialize a Project

Create an empty folder and initialize a `.pstdr` workspace:

```sh
mkdir frost_scythe_addon
cd frost_scythe_addon
pstdr-c init "Frost Scythe Add-On"
```

This creates:
- `pstdr.toml` — Project settings targeting Bedrock `26.60` and `@minecraft/server 2.11.0`.
- `BP/manifest.json` — Ready-to-use Behavior Pack manifest with both `data` and `script` modules and `"min_engine_version": [1, 26, 60]`.
- `src/welcome.pstdr` — A sample welcome script.

---

## Step 2: Create the Item JSON

Create `BP/items/frost_scythe.json`:

```json
{
  "format_version": "1.21.90",
  "minecraft:item": {
    "description": {
      "identifier": "my_pack:frost_scythe"
    },
    "components": {
      "minecraft:icon": "snowball",
      "minecraft:max_stack_size": 1,
      "minecraft:damage": 6
    }
  }
}
```

> **Tip:** We use `"snowball"` as the `minecraft:icon` because it is a valid vanilla texture alias. That way, your Behavior Pack works cleanly without needing a separate Resource Pack (`RP/`).

---

## Step 3: Write the `.pstdr` Script

Create `src/frost_scythe.pstdr`:

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

Notice you do **not** have to edit `BP/items/frost_scythe.json` to add custom components — `bind_target(...)` tells `pstdr-c` to wire it for you automatically!

---

## Step 4: Compile and Audit

Run `convert --workspace` and `audit --workspace`:

```sh
pstdr-c convert --workspace
pstdr-c audit --workspace
```

If everything is valid, `pstdr-c` will:
1. Inject `"pstdr:frost_scythe": {}` into `BP/items/frost_scythe.json`
2. Generate `BP/scripts/generated/frost_scythe.js` (and `build/ts/frost_scythe.ts`)
3. Update `BP/scripts/main.js` and `BP/manifest.json`
4. Pass the Audit Gate with `0` errors.

---

## Step 5: Package as `.mcpack` and Test in Bedrock 26.60

Package the `BP/` folder into a `.mcpack` file:

```sh
cd BP
zip -r ../frost_scythe.mcpack .
cd ..
```

1. Open `frost_scythe.mcpack` (or copy it into your Minecraft Bedrock `behavior_packs` folder).
2. Create a world and enable the **Frost Scythe Add-On** Behavior Pack.
3. In-game, run:
   ```text
   /give @s my_pack:frost_scythe
   ```
4. Hit any mob with the scythe — it will be afflicted with Freezing and Slowness II, and you'll see `❄ Frostbite!` on your actionbar!

---

> [← Previous: Installation](./02-Instalasi.md) | [Next: Code Recipes →](./04-Resep-Kode.md)
