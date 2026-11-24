# Events and Actions

## Supported Events

| Event | Syntax | Scope Variables | Bedrock Bus |
| :--- | :--- | :--- | :--- |
| **Player Join** | `on join of <actor> { ... }` | `<actor>` | `world.afterEvents.playerSpawn` (`ev.initialSpawn`) |
| **Right Click** | `on right click with <item> by <actor> { ... }` | `<actor>` | `afterEvents.itemUse` (or `beforeEvents.itemUse` if `cancel event` is present) |
| **Hit Entity** | `on hit <target> with <item> by <actor> { ... }` | `<target>`, `<actor>` | `world.afterEvents.entityHitEntity` |
| **Break Block** | `on break of <block_id> by <actor> { ... }` | `<actor>`, `block` | `afterEvents.playerBreakBlock` (or `beforeEvents.playerBreakBlock` if `cancel event` is present) |

> **Automatic `beforeEvents` Scheduling:** When `on right click` or `on break` contains `cancel event`, `pstdr-c` subscribes to `beforeEvents` and automatically wraps world-mutating actions (`damage`, `ignite`, `apply`, `send`, `state` writes, expression statements) in `system.run(() => { ... })`.

### Examples

```pstdr
const WAND_ID = "example:wand"
const ORE_ID = "example:ore"

on join of player {
    send "Welcome, {player.name_tag}" to player
}

on right click with WAND_ID by player {
    player.actionbar("Activated")
}

on hit target with WAND_ID by player {
    damage target by 4
    apply slowness 2 to target for 3s
}

on break of ORE_ID by player {
    cancel event
    player.actionbar("Protected block")
}
```

---

## Timers (`after` and `every`)

Timers work both at the top level and nested inside event blocks:
- `after <duration> { ... }` → compiles to `system.runTimeout(..., ticks)`.
- `every <duration> { ... }` → compiles to `system.runInterval(..., ticks)`. Minimum interval is **`50ms`** (`1` tick); smaller intervals fail with `PSTDR-E010`.

```pstdr
on join of player {
    after 2s {
        send "Delayed message" to player
    }
}

every 5s {
    for p in world.players() {
        p.add_tag("ticked")
    }
}
```

---

## Built-in Action Statements

| Action | Syntax | Notes |
| :--- | :--- | :--- |
| **Cancel Event** | `cancel event` | Valid only in `on right click` and `on break` (`PSTDR-E004` otherwise). |
| **Damage** | `damage <target> by <amount>` | Emits `<target>.applyDamage(<amount>)`. |
| **Ignite** | `ignite <target> for <duration>` | Emits `<target>.setOnFire(<seconds>, true)`. |
| **Apply Effect** | `apply <effect> <level> to <target> for <duration>` | `<level>` is 1-based (`PSTDR-E011`); `<effect>` is checked against vanilla spec (`PSTDR-E013`). |
| **Send Message** | `send <message> to <target>` | Emits `<target>.sendMessage(<message>)`. |

---

## `snake_case` Mapping Reference

### Entity & Player Properties

| `.pstdr` | Bedrock API |
| :--- | :--- |
| `name_tag` | `nameTag` |
| `type_id` | `typeId` |
| `is_sneaking` | `isSneaking` |
| `is_sprinting` | `isSprinting` |
| `is_swimming` | `isSwimming` |
| `is_flying` | `isFlying` |
| `is_sleeping` | `isSleeping` |
| `is_on_ground` | `isOnGround` |
| `is_in_water` | `isInWater` |
| `is_climbing` | `isClimbing` |
| `is_falling` | `isFalling` |
| `is_valid` | `isValid` |
| `selected_slot` | `selectedSlotIndex` |
| `head_location` | `getHeadLocation()` |
| `view_direction` | `getViewDirection()` |
| `hp` | `getComponent("minecraft:health")?.currentValue` |
| `max_hp` | `getComponent("minecraft:health")?.effectiveMax` |
| `holding` / `mainhand` | `getComponent("minecraft:equippable")?.getEquipment(EquipmentSlot.Mainhand)` |

### ItemStack Properties (after `.mainhand` / `.holding`)

| `.pstdr` | Bedrock API |
| :--- | :--- |
| `id` / `type_id` | `typeId` |
| `name_tag` | `nameTag` |

### Entity & Player Methods

| `.pstdr` | Bedrock API |
| :--- | :--- |
| `actionbar(msg)` | `onScreenDisplay.setActionBar(msg)` |
| `title(msg)` | `onScreenDisplay.setTitle(msg)` |
| `subtitle(msg)` | `onScreenDisplay.updateSubtitle(msg)` |
| `send_message(msg)` | `sendMessage(msg)` |
| `play_sound(id)` | `playSound(id)` *(sound ID checked against spec)* |
| `add_tag(tag)` / `remove_tag(tag)` / `has_tag(tag)` / `get_tags()` | `addTag` / `removeTag` / `hasTag` / `getTags` |
| `apply_damage(amount)` | `applyDamage(amount)` |
| `apply_impulse(vec)` | `applyImpulse(vec)` |
| `apply_knockback(x, z, h, v)` | `applyKnockback(x, z, h, v)` |
| `run_command(cmd)` | `runCommand(cmd)` |
| `add_effect(id, dur, opts)` / `remove_effect(id)` | `addEffect` / `removeEffect` |
| `set_on_fire(secs, ext)` / `extinguish_fire()` | `setOnFire` / `extinguishFire` |

### Global `world` Helpers

| `.pstdr` | Bedrock API |
| :--- | :--- |
| `world.players()` | `world.getAllPlayers()` |
| `world.send_message(msg)` | `world.sendMessage(msg)` |
| `world.dimension(id)` | `world.getDimension(id)` |
| `world.play_sound(id, loc)` | `world.playSound(id, loc)` |

---

> [← Previous: Language Guide](./04-Language-Guide.md) | [Next: CLI and Diagnostics →](./06-CLI-and-Diagnostics.md)
