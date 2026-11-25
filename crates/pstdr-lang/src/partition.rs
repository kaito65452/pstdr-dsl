use crate::ast::{BindTargetStmt, EventHandler, EventKind, Program, TopLevelItem};
use crate::codegen::{block_contains_cancel, ComponentKind};
use crate::validator::eval_static_string;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetEntry {
    pub identifier: String,
    pub kind: ComponentKind,
    pub json_path: PathBuf,
    pub slug: String,
    pub component_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct TargetRegistry {
    pub by_identifier: BTreeMap<String, TargetEntry>,
}

impl TargetRegistry {
    /// Scans `BP/items/**/*.json` and `BP/blocks/**/*.json` under `workspace_root`
    /// to build a deterministic identifier -> TargetEntry lookup table.
    pub fn discover(workspace_root: &Path) -> Result<Self, String> {
        let mut raw_entries: Vec<(String, ComponentKind, PathBuf)> = Vec::new();

        for sub in ["BP/items", "BP/blocks"] {
            let dir = workspace_root.join(sub);
            if dir.is_dir() {
                let mut json_files = Vec::new();
                collect_json_recursive(&dir, &mut json_files)?;
                json_files.sort();
                for path in json_files {
                    if let Some((id, kind)) = extract_bp_target(&path) {
                        raw_entries.push((id, kind, path));
                    }
                }
            }
        }

        let mut registry = Self::default();
        registry.insert_entries(raw_entries);
        Ok(registry)
    }

    /// Registers any explicit `bind_target(...)` JSON files that were not already discovered.
    pub fn register_bind_targets(
        &mut self,
        workspace_root: &Path,
        bind_targets: &[BindTargetStmt],
    ) {
        let mut extra = Vec::new();
        for bt in bind_targets {
            let path = workspace_root.join(&bt.path);
            if path.is_file() {
                if let Some((id, kind)) = extract_bp_target(&path) {
                    if !self.by_identifier.contains_key(&id) {
                        extra.push((id, kind, path));
                    }
                }
            }
        }
        if !extra.is_empty() {
            self.insert_entries(extra);
        }
    }

    fn insert_entries(&mut self, raw_entries: Vec<(String, ComponentKind, PathBuf)>) {
        // Count local slugs across both existing and new entries to detect collisions.
        let mut local_slug_counts: HashMap<String, usize> = HashMap::new();
        for id in self.by_identifier.keys() {
            *local_slug_counts.entry(local_slug(id)).or_insert(0) += 1;
        }
        for (id, _, _) in &raw_entries {
            if !self.by_identifier.contains_key(id) {
                *local_slug_counts.entry(local_slug(id)).or_insert(0) += 1;
            }
        }

        // If any newly added entry collides with an existing entry's local slug,
        // update the existing entry to use its namespaced slug as well.
        for (id, entry) in &mut self.by_identifier {
            let ls = local_slug(id);
            if local_slug_counts.get(&ls).copied().unwrap_or(0) > 1 {
                let ns_slug = full_slug(id);
                entry.slug = ns_slug.clone();
                entry.component_id = format!("pstdr:{ns_slug}");
            }
        }

        for (id, kind, json_path) in raw_entries {
            if self.by_identifier.contains_key(&id) {
                continue;
            }
            let ls = local_slug(&id);
            let slug = if local_slug_counts.get(&ls).copied().unwrap_or(0) > 1 {
                full_slug(&id)
            } else {
                ls
            };
            let component_id = format!("pstdr:{slug}");
            self.by_identifier.insert(
                id.clone(),
                TargetEntry {
                    identifier: id,
                    kind,
                    json_path,
                    slug,
                    component_id,
                },
            );
        }
    }

    pub fn get(&self, identifier: &str) -> Option<&TargetEntry> {
        self.by_identifier.get(identifier)
    }

    pub fn has_block(&self, identifier: &str) -> bool {
        self.by_identifier
            .get(identifier)
            .is_some_and(|e| e.kind == ComponentKind::Block)
    }

    pub fn has_item(&self, identifier: &str) -> bool {
        self.by_identifier
            .get(identifier)
            .is_some_and(|e| e.kind == ComponentKind::Item)
    }
}

#[derive(Debug, Clone)]
pub struct PartitionedTarget {
    pub entry: TargetEntry,
    pub handlers: Vec<EventHandler>,
}

#[derive(Debug, Clone)]
pub struct PartitionedProgram {
    pub shared: Vec<TopLevelItem>,
    pub items: BTreeMap<String, PartitionedTarget>,
    pub blocks: BTreeMap<String, PartitionedTarget>,
    pub world: Vec<TopLevelItem>,
}

impl PartitionedProgram {
    /// Returns true if this file should be emitted as partitioned modules
    /// (`items/`, `blocks/`, `world/`, `_shared/`, `_registry.js`) rather than a
    /// single legacy `<stem>.js` file.
    pub fn should_partition(&self, bind_target_count: usize) -> bool {
        let target_count = self.items.len() + self.blocks.len();
        if target_count == 0 {
            return false;
        }
        if target_count > 1 {
            return true;
        }
        if !self.world.is_empty() {
            return true;
        }
        // Auto-discovered target without explicit bind_target(...) uses partitioned mode
        // so its JSON wiring and custom component registration are handled automatically.
        bind_target_count == 0
    }
}

/// Partitions a parsed `.pstdr` program into `shared`, `items`, `blocks`, and `world` buckets
/// using the workspace `TargetRegistry` and top-level `const` string folding.
pub fn partition_program(program: &Program, registry: &TargetRegistry) -> PartitionedProgram {
    let mut const_strings: HashMap<String, String> = HashMap::new();
    for item in &program.items {
        if let TopLevelItem::ConstDecl(d) = item {
            if let Some(s) = eval_static_string(&d.value, &const_strings) {
                const_strings.insert(d.name.clone(), s);
            }
        }
    }

    let mut shared = Vec::new();
    let mut items: BTreeMap<String, PartitionedTarget> = BTreeMap::new();
    let mut blocks: BTreeMap<String, PartitionedTarget> = BTreeMap::new();
    let mut world = Vec::new();

    for item in &program.items {
        match item {
            TopLevelItem::ConstDecl(_)
            | TopLevelItem::MutDecl(_)
            | TopLevelItem::StateDecl(_)
            | TopLevelItem::FuncDecl(_)
            | TopLevelItem::UnsafeFuncDecl(_)
            | TopLevelItem::UnsafeVarDecl(_) => {
                shared.push(item.clone());
            }
            TopLevelItem::McFunctionDecl(_) => {
                // Emitted directly to BP/functions/<path>.mcfunction by workspace conversion.
            }
            TopLevelItem::AfterTimer { .. } | TopLevelItem::EveryTimer { .. } => {
                world.push(item.clone());
            }
            TopLevelItem::EventHandler(ev) => {
                let has_cancel = block_contains_cancel(&ev.body);
                match &ev.kind {
                    EventKind::RightClick { item: item_expr, .. } => {
                        if !has_cancel {
                            if let Some(entry) = resolve_target(
                                item_expr,
                                &const_strings,
                                registry,
                                ComponentKind::Item,
                            ) {
                                items
                                    .entry(entry.slug.clone())
                                    .or_insert_with(|| PartitionedTarget {
                                        entry: entry.clone(),
                                        handlers: Vec::new(),
                                    })
                                    .handlers
                                    .push(ev.clone());
                                continue;
                            }
                        }
                        world.push(item.clone());
                    }
                    EventKind::HitTarget {
                        item: item_expr, ..
                    }
                    | EventKind::Consume {
                        item: item_expr, ..
                    } => {
                        if let Some(entry) = resolve_target(
                            item_expr,
                            &const_strings,
                            registry,
                            ComponentKind::Item,
                        ) {
                            items
                                .entry(entry.slug.clone())
                                .or_insert_with(|| PartitionedTarget {
                                    entry: entry.clone(),
                                    handlers: Vec::new(),
                                })
                                .handlers
                                .push(ev.clone());
                        } else {
                            world.push(item.clone());
                        }
                    }
                    EventKind::BlockInteract { block_id, .. }
                    | EventKind::BreakBlock { block_id, .. } => {
                        // Custom block components cannot cancel events; route cancellable
                        // `on interact` / `on break` to world/<stem>.js (beforeEvents).
                        if !has_cancel {
                            if let Some(entry) = resolve_target(
                                block_id,
                                &const_strings,
                                registry,
                                ComponentKind::Block,
                            ) {
                                blocks
                                    .entry(entry.slug.clone())
                                    .or_insert_with(|| PartitionedTarget {
                                        entry: entry.clone(),
                                        handlers: Vec::new(),
                                    })
                                    .handlers
                                    .push(ev.clone());
                                continue;
                            }
                        }
                        world.push(item.clone());
                    }
                    EventKind::BlockTick { block_id } => {
                        if let Some(entry) = resolve_target(
                            block_id,
                            &const_strings,
                            registry,
                            ComponentKind::Block,
                        ) {
                            blocks
                                .entry(entry.slug.clone())
                                .or_insert_with(|| PartitionedTarget {
                                    entry: entry.clone(),
                                    handlers: Vec::new(),
                                })
                                .handlers
                                .push(ev.clone());
                        } else {
                            world.push(item.clone());
                        }
                    }
                    EventKind::PlayerJoin { .. }
                    | EventKind::PlayerLeave { .. }
                    | EventKind::ChatSend { .. }
                    | EventKind::Hurt { .. }
                    | EventKind::Die { .. }
                    | EventKind::ProjectileHit { .. }
                    | EventKind::EntitySpawn { .. }
                    | EventKind::ScriptEvent { .. }
                    | EventKind::EntityTrigger { .. } => {
                        world.push(item.clone());
                    }
                }
            }
        }
    }

    PartitionedProgram {
        shared,
        items,
        blocks,
        world,
    }
}

fn resolve_target<'a>(
    expr: &crate::ast::Expr,
    const_strings: &HashMap<String, String>,
    registry: &'a TargetRegistry,
    expected_kind: ComponentKind,
) -> Option<&'a TargetEntry> {
    let id = eval_static_string(expr, const_strings)?;
    let entry = registry.get(&id)?;
    if entry.kind == expected_kind {
        Some(entry)
    } else {
        None
    }
}

fn collect_json_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory '{}': {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {e}"))?;
        let p = entry.path();
        if p.is_dir() {
            collect_json_recursive(&p, out)?;
        } else if p.extension().and_then(|e| e.to_str()) == Some("json") {
            out.push(p);
        }
    }
    Ok(())
}

fn extract_bp_target(path: &Path) -> Option<(String, ComponentKind)> {
    let raw = fs::read_to_string(path).ok()?;
    let val: Value = serde_json::from_str(&raw).ok()?;
    if let Some(id) = val
        .get("minecraft:item")
        .and_then(|v| v.get("description"))
        .and_then(|v| v.get("identifier"))
        .and_then(|v| v.as_str())
    {
        return Some((id.to_string(), ComponentKind::Item));
    }
    if let Some(id) = val
        .get("minecraft:block")
        .and_then(|v| v.get("description"))
        .and_then(|v| v.get("identifier"))
        .and_then(|v| v.as_str())
    {
        return Some((id.to_string(), ComponentKind::Block));
    }
    None
}

fn sanitize_slug(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn local_slug(identifier: &str) -> String {
    let local = identifier
        .split_once(':')
        .map(|(_, name)| name)
        .unwrap_or(identifier);
    sanitize_slug(local)
}

fn full_slug(identifier: &str) -> String {
    sanitize_slug(identifier)
}
