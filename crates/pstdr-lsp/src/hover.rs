use crate::diagnostics::utf16_col_to_byte_offset;
use crate::state::ServerState;
use lsp_types::{Hover, HoverContents, HoverParams, MarkupContent, MarkupKind};
use pstdr_lang::spec::{map_item_property, map_method, map_property, map_world_method};

pub fn hover(state: &ServerState, params: &HoverParams) -> Option<Hover> {
    let uri = &params.text_document_position_params.text_document.uri;
    let pos = params.text_document_position_params.position;
    let text = state.docs.get(uri)?;
    hover_at(text, pos.line, pos.character)
}

pub fn hover_at(text: &str, line: u32, utf16_col: u32) -> Option<Hover> {
    let line_text = text.lines().nth(line as usize)?;
    let byte_idx = utf16_col_to_byte_offset(line_text, utf16_col);
    let (word, start_byte) = extract_token_at(line_text, byte_idx)?;

    // 1. Duration literal (e.g. 3s, 500ms, 2m, 10t)
    if let Some(dur_md) = describe_duration(word) {
        return Some(mk_hover(dur_md));
    }

    // 2. Member property or method after `.` / `?.`
    let before = line_text[..start_byte].trim_end();
    if before.ends_with('.') {
        let is_world = before
            .trim_end_matches('.')
            .trim_end_matches('?')
            .trim_end()
            .ends_with("world");
        if is_world {
            if let Some(mapped) = map_world_method(word) {
                return Some(mk_hover(format!(
                    "**World Helper**\n\n`world.{word}(...)` → `world.{mapped}(...)`"
                )));
            }
        }
        match word {
            "give_item" => {
                return Some(mk_hover(
                    "**Safe ItemStack Helper**\n\n`.give_item(item_id, amount?, name_tag?)` → `__pstdr_give_item(entity, itemId, amount = 1, nameTag = null)`\n\nConstructs `new ItemStack(...)` safely and inserts it into `minecraft:inventory` (auto-imports `ItemStack`).".to_string(),
                ));
            }
            "spawn_item" => {
                return Some(mk_hover(
                    "**Safe ItemStack Helper**\n\n`.spawn_item(item_id, loc_or_entity, amount?)` → `__pstdr_spawn_item(dim, itemId, loc, amount = 1)`\n\nConstructs `new ItemStack(...)` when given a string ID and spawns it in the dimension (auto-imports `ItemStack`).".to_string(),
                ));
            }
            "get_score" => {
                return Some(mk_hover(
                    "**Scoreboard Helper**\n\n`.get_score(objective)` → `__pstdr_get_score(target, objective)`\n\nReturns the participant's score as a `number`, defaulting to `0` if the objective or participant does not exist.".to_string(),
                ));
            }
            "set_score" => {
                return Some(mk_hover(
                    "**Scoreboard Helper**\n\n`.set_score(objective, value)` → `__pstdr_set_score(target, objective, value)`\n\nSets the participant's score, automatically creating the objective if missing.".to_string(),
                ));
            }
            "add_score" => {
                return Some(mk_hover(
                    "**Scoreboard Helper**\n\n`.add_score(objective, delta)` → `__pstdr_add_score(target, objective, delta)`\n\nAdds `delta` to the participant's score, automatically creating the objective if missing.".to_string(),
                ));
            }
            _ => {}
        }
        if let Some(mapped) = map_method(word) {
            return Some(mk_hover(format!(
                "**Bedrock Method Helper**\n\n`.{word}(...)` → `.{mapped}(...)`"
            )));
        }
        if let Some(mapped) = map_property(word) {
            return Some(mk_hover(format!(
                "**Bedrock Property Helper**\n\n`.{word}` → `.{mapped}`"
            )));
        }
        if let Some(mapped) = map_item_property(word) {
            return Some(mk_hover(format!(
                "**ItemStack Property Helper**\n\n`.{word}` → `.{mapped}`"
            )));
        }
    }

    // 3. Declared `state` variable or `state` keyword
    if word == "state" || is_declared_state(text, word) {
        let name = if word == "state" { "<name>" } else { word };
        return Some(mk_hover(format!(
            "**Persistent State (`DynamicProperty`)**\n\n\
             Reads compile to `owner.getDynamicProperty(\"pstdr:{name}\")`.\n\n\
             Writes compile to `owner.setDynamicProperty(\"pstdr:{name}\", value)`."
        )));
    }

    // 4. Core DSL keywords
    let kw_doc = match word {
        "apply" => Some("**`apply <effect> <level> to <target> for <duration>`**\n\nApplies a status effect using 1-based levels (automatically converted to Bedrock 0-based `amplifier`)."),
        "cancel" => Some("**`cancel event`**\n\nValid inside `on right click`, `on break`, `on interact`, and `on chat` *(Beta API only)*. Automatically switches to `beforeEvents` and schedules mutations in `system.run(...)`."),
        "chat" => Some("**`on chat <msg> by <player> { ... }`** `[Beta API only — requires api = \"...-beta\"]`\n\nCompiles to `world.beforeEvents.chatSend` (cancellable, auto `system.run(...)` for mutations). Requires `@addon(target = \"26.60\", api = \"2.11.0-beta\")` and the **Beta APIs** world experiment (`PSTDR-E018` on stable APIs)."),
        "every" => Some("**`every <duration> { ... }`**\n\nCompiles to `system.runInterval(..., ticks)`. Minimum interval is `50ms` (`1` tick)."),
        "after" => Some("**`after <duration> { ... }`**\n\nCompiles to `system.runTimeout(..., ticks)`."),
        "unsafe" => Some("**`unsafe`**\n\nIsolates raw JavaScript inside a `.pstdr` file without DSL keyword restrictions."),
        "bind_target" => Some("**`bind_target(\"BP/...\")`**\n\nExplicitly binds a Behavior Pack item or block JSON file for automatic custom component wiring."),
        "command" => Some("**`command [on <target>] \"<cmd>\"`**\n\nExecutes a synchronous Bedrock command via `.runCommand(...)` (on `world.getDimension(\"overworld\")` by default, or on `<target>`). Automatically deferred via `system.run(...)` inside `beforeEvents` handlers."),
        "run_fn" => Some("**`run_fn [on <target>] \"<path>\"`**\n\nCompiles to `.runCommand(\"function <path>\")`. Validated against `BP/functions/<path>.mcfunction` and inline `mcfunction` blocks (`PSTDR-E017`). Automatically deferred via `system.run(...)` inside `beforeEvents` handlers."),
        "mcfunction" => Some("**`mcfunction \"<path>\" { ... }`**\n\nTop-level inline `.mcfunction` declaration emitted to `BP/functions/<path>.mcfunction` during workspace conversion (stripping `//` comments and blank lines)."),
        "script_event" => Some("**`on script_event \"<ns>:<id>\" { ... }`**\n\nCompiles to `system.afterEvents.scriptEventReceive.subscribe(..., { namespaces: [\"<ns>\"] })`. Exposes `event.id`, `event.message`, `source` (`event.sourceEntity`), and `source_block` (`event.sourceBlock`)."),
        "entity_trigger" => Some("**`on entity_trigger \"<event_id>\" [for \"<entity_type>\"] { ... }`**\n\nCompiles to `world.afterEvents.dataDrivenEntityTrigger.subscribe(..., { eventTypes: [\"<event_id>\"], ... })`. Exposes `entity` (`event.entity`) and `event_id` (`event.eventId`)."),
        _ => None,
    };

    kw_doc.map(|s| mk_hover(s.to_string()))
}

fn mk_hover(markdown: String) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: markdown,
        }),
        range: None,
    }
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.'
}

fn extract_token_at(line: &str, byte_idx: usize) -> Option<(&str, usize)> {
    let bytes = line.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut idx = byte_idx.min(line.len());
    if idx == line.len() && idx > 0 {
        idx -= 1;
    }
    // Snap to char boundary
    while idx > 0 && !line.is_char_boundary(idx) {
        idx -= 1;
    }

    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';

    // Check if cursor is on a duration literal like `1.5s`
    let mut dur_start = idx;
    while dur_start > 0 {
        let prev = line[..dur_start].chars().next_back()?;
        if is_token_char(prev) {
            dur_start -= prev.len_utf8();
        } else {
            break;
        }
    }
    let mut dur_end = idx;
    while dur_end < line.len() {
        let next = line[dur_end..].chars().next()?;
        if is_token_char(next) {
            dur_end += next.len_utf8();
        } else {
            break;
        }
    }
    let dur_cand = &line[dur_start..dur_end];
    if describe_duration(dur_cand).is_some() {
        return Some((dur_cand, dur_start));
    }

    // Standard identifier extraction
    let mut start = idx;
    if !line[start..].chars().next().is_some_and(is_word) {
        if start == 0 {
            return None;
        }
        let prev = line[..start].chars().next_back()?;
        if !is_word(prev) {
            return None;
        }
        start -= prev.len_utf8();
    }
    while start > 0 {
        let prev = line[..start].chars().next_back()?;
        if is_word(prev) {
            start -= prev.len_utf8();
        } else {
            break;
        }
    }
    let mut end = idx;
    while end < line.len() {
        let next = line[end..].chars().next()?;
        if is_word(next) {
            end += next.len_utf8();
        } else {
            break;
        }
    }
    if start < end {
        Some((&line[start..end], start))
    } else {
        None
    }
}

fn describe_duration(token: &str) -> Option<String> {
    let (num_str, unit) = if let Some(n) = token.strip_suffix("ms") {
        (n, "ms")
    } else if let Some(n) = token.strip_suffix('s') {
        (n, "s")
    } else if let Some(n) = token.strip_suffix('m') {
        (n, "m")
    } else {
        let n = token.strip_suffix('t')?;
        (n, "t")
    };

    if num_str.is_empty() || !num_str.chars().next()?.is_ascii_digit() {
        return None;
    }
    let val: f64 = num_str.parse().ok()?;
    let ms = match unit {
        "ms" => val,
        "s" => val * 1000.0,
        "m" => val * 60_000.0,
        "t" => val * 50.0,
        _ => return None,
    };
    let ticks = (ms / 50.0).round() as u64;
    Some(format!(
        "**Duration Literal**\n\n`{token}` = **{ticks} ticks** ({} ms)",
        if ms.fract() == 0.0 {
            format!("{}", ms as i64)
        } else {
            format!("{ms}")
        }
    ))
}

fn is_declared_state(text: &str, name: &str) -> bool {
    let needle = format!("state {name}[");
    text.lines().any(|l| l.trim_start().starts_with(&needle))
}
