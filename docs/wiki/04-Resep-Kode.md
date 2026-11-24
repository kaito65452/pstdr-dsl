# Code Recipes

Copy-pasteable `.pstdr` patterns for common Minecraft Bedrock 26.60 add-on features.

---

## 1. Welcome Message When a Player Joins (`on join`)

```pstdr
on join of player {
    send "§aWelcome to the server, {player.name_tag}!" to player
    player.title("§bFrost World")
    player.subtitle("§7Survive the cold")
}
```

---

## 2. Right-Click Item Ability (`on right click`)

```pstdr
const WAND_ID = "my_pack:ice_wand"

on right click with WAND_ID by player {
    if player is sneaking {
        player.play_sound("random.orb")
        apply speed 2 to player for 5s
        player.actionbar("§bDash Boost!")
    } else {
        player.actionbar("§7Sneak + right-click to dash.")
    }
}
```

---

## 3. Custom Weapon on Hit (`on hit`)

Bedrock 26.60 includes `freezing` and `slowness` effects. Levels in `.pstdr` are 1-based (`1` = Level I, `2` = Level II):

```pstdr
const SCYTHE_ID = "my_pack:frost_scythe"

on hit target with SCYTHE_ID by player {
    damage target by 4
    apply freezing 1 to target for 4s
    apply slowness 2 to target for 4s
    player.play_sound("frostbite")
}
```

---

## 4. Block Break & Protection (`on break` + `cancel event`)

```pstdr
on break of "minecraft:diamond_ore" by player {
    if not player.has_tag("miner") {
        cancel event
        player.actionbar("§cYou need the 'miner' tag to break this!")
    }
}
```

---

## 5. Persistent Player Data (`state`)

Use `state` to store per-player data automatically via Bedrock Dynamic Properties:

```pstdr
state mana[player] = 100

const STAFF_ID = "my_pack:mana_staff"

on right click with STAFF_ID by player {
    if mana[player] >= 20 {
        mana[player] -= 20
        player.actionbar("§bCast spell! Mana left: {mana[player]}")
    } else {
        player.actionbar("§cNot enough mana!")
    }
}
```

---

## 6. Timers (`after` and `every`)

- **`after`** runs once after a delay (`50ms`, `3s`, `2m`).
- **`every`** repeats on an interval (must be at least `50ms` / 1 tick).

```pstdr
on join of player {
    after 3s {
        send "§eTip: Craft a Frost Scythe to slow enemies!" to player
    }
}

every 5s {
    for p in world.players() {
        if p is in_water {
            apply freezing 1 to p for 2s
        }
    }
}
```

---

## 7. Regular Functions (`func`) vs Raw JS (`unsafe`)

Use `func` for safe reusable `.pstdr` helpers. When you need direct `@minecraft/server` APIs that aren't wrapped yet, isolate them inside a top-level `unsafe` function or variable:

```pstdr
func heal_player(p) {
    apply regeneration 2 to p for 3s
}

unsafe teleport_up(entity, blocks) {
    const loc = entity.location;
    entity.teleport({ x: loc.x, y: loc.y + blocks, z: loc.z });
}

on right click with "minecraft:feather" by player {
    heal_player(player)
    teleport_up(player, 3)
}
```

---

## 8. Common `snake_case` Properties & Methods

`.pstdr` lets you write clean `snake_case` names and converts them to Bedrock's `@minecraft/server` API automatically:

| `.pstdr` | Bedrock API Equivalent |
| :--- | :--- |
| `player.name_tag` | `player.nameTag` |
| `target.type_id` | `target.typeId` |
| `player.hp` / `player.max_hp` | Health component `currentValue` / `effectiveMax` |
| `player.holding` / `player.mainhand` | Equipped Mainhand `ItemStack` |
| `player.is_sneaking` / `player.is_on_ground` | `player.isSneaking` / `player.isOnGround` |
| `player.actionbar("msg")` | `player.onScreenDisplay.setActionBar("msg")` |
| `player.title("msg")` / `player.subtitle("msg")` | `setTitle` / `updateSubtitle` |
| `player.send_message("msg")` | `player.sendMessage("msg")` |
| `player.play_sound("id")` | `player.playSound("id")` |
| `player.add_tag("t")` / `player.has_tag("t")` | `addTag` / `hasTag` |
| `target.apply_damage(5)` | `target.applyDamage(5)` |
| `target.apply_knockback(x, z, h, v)` | `target.applyKnockback(x, z, h, v)` |
| `player.run_command("...")` | `player.runCommand("...")` |
| `world.players()` | `world.getAllPlayers()` |

---

> [← Previous: Your First Add-On](./03-Addon-Pertama.md) | [Next: Troubleshooting Errors →](./05-Mengatasi-Error.md)
