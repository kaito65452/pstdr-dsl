use crate::diagnostics::utf16_col_to_byte_offset;
use crate::state::ServerState;
use lsp_types::{CompletionItem, CompletionItemKind, CompletionParams, InsertTextFormat};

const MEMBER_PROPERTIES: &[(&str, &str)] = &[
    ("name_tag", "nameTag"),
    ("type_id", "typeId"),
    ("hp", "getComponent(\"minecraft:health\")?.currentValue"),
    ("max_hp", "getComponent(\"minecraft:health\")?.effectiveMax"),
    ("selected_slot", "selectedSlotIndex"),
    ("head_location", "getHeadLocation()"),
    ("view_direction", "getViewDirection()"),
    ("holding", "getComponent(\"minecraft:equippable\")?.getEquipment(EquipmentSlot.Mainhand)"),
    ("mainhand", "getComponent(\"minecraft:equippable\")?.getEquipment(EquipmentSlot.Mainhand)"),
    ("location", "location"),
    ("dimension", "dimension"),
    ("is_sneaking", "isSneaking"),
    ("is_sprinting", "isSprinting"),
    ("is_swimming", "isSwimming"),
    ("is_flying", "isFlying"),
    ("is_sleeping", "isSleeping"),
    ("is_on_ground", "isOnGround"),
    ("is_in_water", "isInWater"),
    ("is_climbing", "isClimbing"),
    ("is_falling", "isFalling"),
    ("is_valid", "isValid"),
];

const MEMBER_METHODS: &[(&str, &str, &str)] = &[
    ("actionbar", "actionbar(${1:\"msg\"})", "onScreenDisplay.setActionBar(msg)"),
    ("title", "title(${1:\"title\"})", "onScreenDisplay.setTitle(msg)"),
    ("subtitle", "subtitle(${1:\"subtitle\"})", "onScreenDisplay.updateSubtitle(msg)"),
    ("send_message", "send_message(${1:\"msg\"})", "sendMessage(msg)"),
    ("play_sound", "play_sound(\"${1:random.orb}\")", "playSound(id)"),
    ("add_tag", "add_tag(\"${1:tag}\")", "addTag(tag)"),
    ("remove_tag", "remove_tag(\"${1:tag}\")", "removeTag(tag)"),
    ("has_tag", "has_tag(\"${1:tag}\")", "hasTag(tag)"),
    ("get_tags", "get_tags()", "getTags()"),
    ("apply_damage", "apply_damage(${1:4})", "applyDamage(amount)"),
    ("apply_impulse", "apply_impulse(${1:{ x = 0, y = 1, z = 0 }})", "applyImpulse(vec)"),
    ("apply_knockback", "apply_knockback(${1:0}, ${2:0}, ${3:1}, ${4:0.5})", "applyKnockback(x, z, h, v)"),
    ("run_command", "run_command(\"${1:say hi}\")", "runCommand(cmd)"),
    ("add_effect", "add_effect(\"${1:speed}\", ${2:100})", "addEffect(id, ticks, opts)"),
    ("remove_effect", "remove_effect(\"${1:speed}\")", "removeEffect(id)"),
    ("set_on_fire", "set_on_fire(${1:3}, true)", "setOnFire(seconds, useEffects)"),
    ("extinguish_fire", "extinguish_fire()", "extinguishFire()"),
    ("spawn_particle", "spawn_particle(\"${1:minecraft:totem_particle}\", ${2:loc})", "spawnParticle(id, loc)"),
    ("spawn_entity", "spawn_entity(\"${1:minecraft:zombie}\", ${2:loc})", "spawnEntity(id, loc)"),
    ("spawn_item", "spawn_item(\"${1:minecraft:diamond}\", ${2:loc}, ${3:1})", "__pstdr_spawn_item(dim, itemId, loc, amount?)"),
    ("give_item", "give_item(\"${1:minecraft:diamond}\", ${2:1})", "__pstdr_give_item(entity, itemId, amount?, nameTag?)"),
    ("get_score", "get_score(\"${1:objective}\")", "__pstdr_get_score(target, objective)"),
    ("set_score", "set_score(\"${1:objective}\", ${2:0})", "__pstdr_set_score(target, objective, value)"),
    ("add_score", "add_score(\"${1:objective}\", ${2:1})", "__pstdr_add_score(target, objective, delta)"),
    ("get_entities", "get_entities(${1:{ maxDistance = 8 }})", "getEntities(opts)"),
    ("get_players", "get_players(${1:{ maxDistance = 8 }})", "getPlayers(opts)"),
    ("teleport", "teleport(${1:loc})", "teleport(loc)"),
    ("kill", "kill()", "kill()"),
    ("players", "players()", "world.getAllPlayers()"),
];

const PREDICATES: &[(&str, &str)] = &[
    ("sneaking", "subject.isSneaking"),
    ("sprinting", "subject.isSprinting"),
    ("swimming", "subject.isSwimming"),
    ("flying", "subject.isFlying"),
    ("sleeping", "subject.isSleeping"),
    ("on_ground", "subject.isOnGround"),
    ("in_water", "subject.isInWater"),
    ("holding", "holding \"namespace:item\""),
];

const TOP_LEVEL_SNIPPETS: &[(&str, &str, &str)] = &[
    ("@addon", "@addon(target = \"26.60\", api = \"2.11.0\")", "Target Bedrock & Script API directive"),
    ("bind_target", "bind_target(\"BP/items/${1:item}.json\")", "Bind a BP item or block JSON file"),
    ("const", "const ${1:NAME} = ${2:value}", "Immutable binding"),
    ("mut", "mut ${1:counter} = ${2:0}", "Mutable binding"),
    ("state", "state ${1:mana}[player] = ${2:100}", "Persistent DynamicProperty state"),
    ("func", "func ${1:helper}(${2:player}) {\n    $0\n}", "Safe .pstdr function"),
    ("unsafe", "unsafe ${1:raw_fn}(${2:arg}) {\n    $0\n}", "Isolated raw JavaScript function"),
    ("on join", "on join of ${1:player} {\n    $0\n}", "Player initial join event"),
    ("on leave", "on leave of ${1:player} {\n    $0\n}", "Player leave event (beforeEvents, auto system.run)"),
    ("on chat", "on chat ${1:msg} by ${2:player} {\n    $0\n}", "Chat send event (cancellable beforeEvents, auto system.run)"),
    ("on right click", "on right click with ${1:\"example:wand\"} by ${2:player} {\n    $0\n}", "Item right-click / use event"),
    ("on hit", "on hit ${1:target} with ${2:\"example:wand\"} by ${3:player} {\n    $0\n}", "Entity hit with item event"),
    ("on break", "on break of ${1:\"minecraft:stone\"} by ${2:player} {\n    $0\n}", "Block break event (cancellable)"),
    ("on hurt", "on hurt ${1:target} by ${2:attacker} {\n    $0\n}", "Entity hurt event"),
    ("on die", "on die of ${1:victim} by ${2:killer} {\n    $0\n}", "Entity death event"),
    ("on consume", "on consume ${1:\"example:potion\"} by ${2:player} {\n    $0\n}", "Item consume completion event"),
    ("on interact", "on interact of ${1:\"example:altar\"} by ${2:player} {\n    $0\n}", "Block interact event (cancellable or custom component)"),
    ("on tick", "on tick of ${1:\"example:altar\"} {\n    $0\n}", "Custom block tick event"),
    ("on projectile hit", "on projectile hit ${1:victim} with ${2:\"example:spear\"} by ${3:shooter} {\n    $0\n}", "Projectile hit entity event"),
    ("on spawn", "on spawn of ${1:entity} type ${2:\"minecraft:zombie\"} {\n    $0\n}", "Entity spawn event"),
    ("after", "after ${1:2s} {\n    $0\n}", "One-shot delayed timer (system.runTimeout)"),
    ("every", "every ${1:1s} {\n    $0\n}", "Repeating interval timer (system.runInterval)"),
    ("spawn particle", "spawn particle \"${1:minecraft:totem_particle}\" at ${2:player}", "Spawn particle at entity or coordinates"),
    ("apply", "apply ${1:speed} ${2:1} to ${3:player} for ${4:5s}", "Apply 1-based status effect"),
    ("damage", "damage ${1:target} by ${2:4}", "Deal damage to target entity"),
    ("ignite", "ignite ${1:target} for ${2:3s}", "Set target entity on fire"),
    ("send", "send \"${1:message}\" to ${2:player}", "Send chat message to player"),
    ("cancel event", "cancel event", "Cancel a cancellable beforeEvent"),
];

pub fn complete(state: &ServerState, params: &CompletionParams) -> Vec<CompletionItem> {
    let uri = &params.text_document_position.text_document.uri;
    let pos = params.text_document_position.position;
    let Some(text) = state.docs.get(uri) else {
        return Vec::new();
    };
    complete_at(text, pos.line, pos.character, &state.spec)
}

pub fn complete_at(
    text: &str,
    line: u32,
    utf16_col: u32,
    spec: &pstdr_lang::spec::BedrockSpec,
) -> Vec<CompletionItem> {
    let line_text = text.lines().nth(line as usize).unwrap_or("");
    let byte_idx = utf16_col_to_byte_offset(line_text, utf16_col);
    let prefix = &line_text[..byte_idx];

    // 1. Inside play_sound("...
    if is_inside_play_sound(prefix) {
        let mut sounds: Vec<&String> = spec.sounds.iter().collect();
        sounds.sort();
        return sounds
            .into_iter()
            .map(|s| CompletionItem {
                label: s.clone(),
                kind: Some(CompletionItemKind::VALUE),
                detail: Some("Vanilla Bedrock sound ID".to_string()),
                ..Default::default()
            })
            .collect();
    }

    // 2. After `apply `
    if is_after_apply(prefix) {
        let mut effects: Vec<&String> = spec.effects.iter().collect();
        effects.sort();
        return effects
            .into_iter()
            .map(|e| CompletionItem {
                label: e.clone(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some("Vanilla Bedrock status effect".to_string()),
                ..Default::default()
            })
            .collect();
    }

    // 3. After `is ` or `is not `
    if is_after_predicate_keyword(prefix) {
        return PREDICATES
            .iter()
            .map(|(label, detail)| CompletionItem {
                label: (*label).to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some((*detail).to_string()),
                ..Default::default()
            })
            .collect();
    }

    // 4. After `.` or `?.`
    if is_after_member_dot(prefix) {
        let mut items = Vec::with_capacity(MEMBER_PROPERTIES.len() + MEMBER_METHODS.len());
        for (prop, api) in MEMBER_PROPERTIES {
            items.push(CompletionItem {
                label: (*prop).to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail: Some(format!("-> {api}")),
                ..Default::default()
            });
        }
        for (name, snippet, api) in MEMBER_METHODS {
            items.push(CompletionItem {
                label: (*name).to_string(),
                kind: Some(CompletionItemKind::METHOD),
                detail: Some(format!("-> {api}")),
                insert_text: Some((*snippet).to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            });
        }
        return items;
    }

    // 5. Top-level / general keywords and event snippets
    TOP_LEVEL_SNIPPETS
        .iter()
        .map(|(label, snippet, detail)| CompletionItem {
            label: (*label).to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some((*detail).to_string()),
            insert_text: Some((*snippet).to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        })
        .collect()
}

fn is_inside_play_sound(prefix: &str) -> bool {
    for needle in ["play_sound(\"", "playSound(\""] {
        if let Some(pos) = prefix.rfind(needle) {
            let after = &prefix[pos + needle.len()..];
            if !after.contains('"') {
                return true;
            }
        }
    }
    false
}

fn is_after_apply(prefix: &str) -> bool {
    let trimmed_start = prefix.trim_start();
    if let Some(rest) = trimmed_start.strip_prefix("apply ") {
        // Still typing the effect name (no second token yet)
        return !rest.trim_start().contains(char::is_whitespace);
    }
    false
}

fn is_after_predicate_keyword(prefix: &str) -> bool {
    let trimmed = prefix.trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_');
    trimmed.ends_with(" is ") || trimmed.ends_with(" is not ")
}

fn is_after_member_dot(prefix: &str) -> bool {
    let trimmed = prefix.trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_');
    trimmed.ends_with('.')
}
