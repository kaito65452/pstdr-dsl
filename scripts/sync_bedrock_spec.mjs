#!/usr/bin/env node
/**
 * Deterministic Bulk Spec Sync for PackStudio DSL (.pstdr).
 *
 * Extracts Stable vs. Beta `@minecraft/server` APIs (events, classes, methods,
 * properties, read-only flags) and official vanilla IDs (items, blocks, effects)
 * directly from Mojang's official `Mojang/bedrock-samples` metadata (`main` vs
 * `preview` branches), merging cleanly into `data/spec/bedrock_api_latest.json`
 * without losing hand-curated `sounds`, `item_textures`, or `terrain_textures`.
 *
 * Uses only built-in Node.js APIs (zero external npm dependencies).
 *
 * Usage:
 *   node scripts/sync_bedrock_spec.mjs
 *   node scripts/sync_bedrock_spec.mjs --out data/spec/bedrock_api_latest.json
 *   node scripts/sync_bedrock_spec.mjs --offline <fixture_dir> [--out <path>]
 */

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import zlib from "node:zlib";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, "..");
const DEFAULT_OUT = path.join(REPO_ROOT, "data/spec/bedrock_api_latest.json");

const GITHUB_API_BASE = "https://api.github.com/repos/Mojang/bedrock-samples/contents";
const GITHUB_RAW_BASE = "https://raw.githubusercontent.com/Mojang/bedrock-samples";
const NPM_SERVER_META = "https://registry.npmjs.org/@minecraft/server";

function parseArgs(argv) {
  const args = {
    offlineDir: null,
    outPath: DEFAULT_OUT,
    checkNpm: false,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--offline" && i + 1 < argv.length) {
      args.offlineDir = path.resolve(argv[++i]);
    } else if (arg === "--out" && i + 1 < argv.length) {
      args.outPath = path.resolve(argv[++i]);
    } else if (arg === "--check-npm") {
      args.checkNpm = true;
    } else if (arg === "--help" || arg === "-h") {
      console.log(
        "Usage: node scripts/sync_bedrock_spec.mjs [--offline <dir>] [--out <path>] [--check-npm]"
      );
      process.exit(0);
    }
  }
  return args;
}

function parseSemver(verStr) {
  const m = /^(\d+)\.(\d+)\.(\d+)(?:-([a-zA-Z0-9.-]+))?$/.exec(verStr);
  if (!m) return null;
  return {
    major: Number(m[1]),
    minor: Number(m[2]),
    patch: Number(m[3]),
    prerelease: m[4] || null,
  };
}

function compareSemver(a, b) {
  if (a.major !== b.major) return a.major - b.major;
  if (a.minor !== b.minor) return a.minor - b.minor;
  if (a.patch !== b.patch) return a.patch - b.patch;
  if (!a.prerelease && b.prerelease) return 1;
  if (a.prerelease && !b.prerelease) return -1;
  return String(a.prerelease || "").localeCompare(String(b.prerelease || ""));
}

function pickLatestBindingsFile(names, prefix, wantBeta) {
  const re = new RegExp(`^${prefix}_(\\d+\\.\\d+\\.\\d+(?:-beta)?)\\.json$`);
  const candidates = [];
  for (const name of names) {
    const m = re.exec(name);
    if (!m) continue;
    const isBeta = m[1].endsWith("-beta");
    if (isBeta !== wantBeta) continue;
    const sv = parseSemver(m[1]);
    if (sv) candidates.push({ name, version: m[1], sv });
  }
  candidates.sort((a, b) => compareSemver(a.sv, b.sv));
  return candidates.length > 0 ? candidates[candidates.length - 1] : null;
}

async function fetchJson(url) {
  const res = await fetch(url, {
    headers: {
      "User-Agent": "pstdr-spec-sync/1.0",
      Accept: "application/json",
    },
  });
  if (!res.ok) {
    throw new Error(`HTTP ${res.status} fetching ${url}`);
  }
  const buf = Buffer.from(await res.arrayBuffer());
  const isGzip = buf.length >= 2 && buf[0] === 0x1f && buf[1] === 0x8b;
  const text = isGzip ? zlib.gunzipSync(buf).toString("utf8") : buf.toString("utf8");
  return JSON.parse(text);
}

function readJsonFile(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function findOfflineBindings(dir, subdir, wantBeta) {
  const direct = path.join(dir, subdir, "server-bindings.json");
  if (fs.existsSync(direct)) {
    return { path: direct, version: wantBeta ? "offline-beta" : "offline-stable" };
  }
  const mcDir = path.join(dir, subdir, "metadata/script_modules/@minecraft");
  if (fs.existsSync(mcDir)) {
    const names = fs.readdirSync(mcDir);
    const picked = pickLatestBindingsFile(names, "server-bindings", wantBeta);
    if (picked) {
      return { path: path.join(mcDir, picked.name), version: picked.version };
    }
  }
  throw new Error(
    `Offline fixture missing ${subdir} server-bindings JSON in '${dir}'.`
  );
}

function findOfflineVanillaFile(dir, fileName) {
  const candidates = [
    path.join(dir, "vanilladata", fileName),
    path.join(dir, "main/metadata/vanilladata_modules", fileName),
    path.join(dir, "stable/metadata/vanilladata_modules", fileName),
    path.join(dir, fileName),
  ];
  for (const c of candidates) {
    if (fs.existsSync(c)) return c;
  }
  return null;
}

function extractClassMembers(bindingsJson) {
  const map = new Map();
  const classes = Array.isArray(bindingsJson.classes) ? bindingsJson.classes : [];
  for (const cls of classes) {
    if (!cls || typeof cls.name !== "string") continue;
    const members = new Set();
    for (const prop of cls.properties || []) {
      if (prop && typeof prop.name === "string") {
        members.add(prop.name);
      }
    }
    for (const fn of cls.functions || []) {
      if (fn && typeof fn.name === "string") {
        members.add(fn.name);
      }
    }
    map.set(cls.name, members);
  }
  return map;
}

function extractEventsFromClass(bindingsJson, className) {
  const classes = Array.isArray(bindingsJson.classes) ? bindingsJson.classes : [];
  const cls = classes.find((c) => c && c.name === className);
  if (!cls || !Array.isArray(cls.properties)) return [];
  return cls.properties
    .filter((p) => p && typeof p.name === "string")
    .map((p) => ({
      name: p.name,
      signalType: p.type && typeof p.type.name === "string" ? p.type.name : null,
    }));
}

function isSignalReadOnly(signalClass) {
  if (!signalClass || !Array.isArray(signalClass.functions)) return false;
  const sub = signalClass.functions.find((f) => f && f.name === "subscribe");
  if (!sub || !Array.isArray(sub.arguments)) return false;
  for (const arg of sub.arguments) {
    const priv = arg?.type?.closure_type?.call_privilege?.name;
    if (typeof priv === "string") {
      const lower = priv.toLowerCase();
      if (
        lower === "restricted_execution" ||
        lower === "read_only" ||
        lower === "read-only"
      ) {
        return true;
      }
    }
  }
  return false;
}

function extractReadOnlyEvents(bindingsJson) {
  const classes = Array.isArray(bindingsJson.classes) ? bindingsJson.classes : [];
  const byName = new Map(classes.map((c) => [c.name, c]));
  const readOnly = new Set();
  for (const eventClassName of ["WorldBeforeEvents", "WorldAfterEvents"]) {
    const events = extractEventsFromClass(bindingsJson, eventClassName);
    for (const ev of events) {
      if (ev.signalType && isSignalReadOnly(byName.get(ev.signalType))) {
        readOnly.add(ev.name);
      }
    }
  }
  return readOnly;
}

function normalizeVanillaIds(vanillaJson) {
  if (!vanillaJson || !Array.isArray(vanillaJson.data_items)) return [];
  const out = new Set();
  for (const item of vanillaJson.data_items) {
    const raw = typeof item === "string" ? item : item?.name;
    if (typeof raw !== "string" || !raw.trim()) continue;
    const norm = raw.trim().replace(/^minecraft:/i, "").toLowerCase();
    if (norm) out.add(norm);
  }
  return [...out].sort();
}

function sortedUnique(iterable) {
  return [...new Set(iterable)].sort((a, b) => a.localeCompare(b));
}

async function loadMetadata(args) {
  if (args.offlineDir) {
    const stableFile = findOfflineBindings(args.offlineDir, "stable", false);
    const betaFile = findOfflineBindings(args.offlineDir, "beta", true);
    const itemsFile = findOfflineVanillaFile(args.offlineDir, "mojang-items.json");
    const blocksFile = findOfflineVanillaFile(args.offlineDir, "mojang-blocks.json");
    const effectsFile = findOfflineVanillaFile(args.offlineDir, "mojang-effects.json");
    return {
      stableVersion: stableFile.version,
      betaVersion: betaFile.version,
      stableBindings: readJsonFile(stableFile.path),
      betaBindings: readJsonFile(betaFile.path),
      vanillaItemsJson: itemsFile ? readJsonFile(itemsFile) : null,
      vanillaBlocksJson: blocksFile ? readJsonFile(blocksFile) : null,
      vanillaEffectsJson: effectsFile ? readJsonFile(effectsFile) : null,
    };
  }

  const [mainList, previewList] = await Promise.all([
    fetchJson(`${GITHUB_API_BASE}/metadata/script_modules/@minecraft?ref=main`),
    fetchJson(`${GITHUB_API_BASE}/metadata/script_modules/@minecraft?ref=preview`),
  ]);

  const mainNames = mainList.map((e) => e.name);
  const previewNames = previewList.map((e) => e.name);

  const stablePick = pickLatestBindingsFile(mainNames, "server-bindings", false);
  const betaPick =
    pickLatestBindingsFile(previewNames, "server-bindings", true) ||
    pickLatestBindingsFile(mainNames, "server-bindings", true);

  if (!stablePick) {
    throw new Error("Could not find stable server-bindings_*.json on Mojang/bedrock-samples main.");
  }
  if (!betaPick) {
    throw new Error("Could not find beta server-bindings_*-beta.json on Mojang/bedrock-samples preview.");
  }

  const [
    stableBindings,
    betaBindings,
    vanillaItemsJson,
    vanillaBlocksJson,
    vanillaEffectsJson,
  ] = await Promise.all([
    fetchJson(
      `${GITHUB_RAW_BASE}/main/metadata/script_modules/@minecraft/${stablePick.name}`
    ),
    fetchJson(
      `${GITHUB_RAW_BASE}/preview/metadata/script_modules/@minecraft/${betaPick.name}`
    ),
    fetchJson(`${GITHUB_RAW_BASE}/main/metadata/vanilladata_modules/mojang-items.json`),
    fetchJson(`${GITHUB_RAW_BASE}/main/metadata/vanilladata_modules/mojang-blocks.json`),
    fetchJson(`${GITHUB_RAW_BASE}/main/metadata/vanilladata_modules/mojang-effects.json`),
  ]);

  return {
    stableVersion: stablePick.version,
    betaVersion: betaPick.version,
    stableBindings,
    betaBindings,
    vanillaItemsJson,
    vanillaBlocksJson,
    vanillaEffectsJson,
  };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const existing = fs.existsSync(args.outPath) ? readJsonFile(args.outPath) : {};

  const meta = await loadMetadata(args);

  // 1. Events: Stable vs Beta
  const stableBefore = extractEventsFromClass(meta.stableBindings, "WorldBeforeEvents").map(
    (e) => e.name
  );
  const stableAfter = extractEventsFromClass(meta.stableBindings, "WorldAfterEvents").map(
    (e) => e.name
  );
  const stableBeforeSet = new Set(stableBefore);
  const stableAfterSet = new Set(stableAfter);

  const betaBefore = extractEventsFromClass(meta.betaBindings, "WorldBeforeEvents").map(
    (e) => e.name
  );
  const betaAfter = extractEventsFromClass(meta.betaBindings, "WorldAfterEvents").map(
    (e) => e.name
  );

  const betaOnlyEvents = new Set();
  for (const ev of betaBefore) {
    if (!stableBeforeSet.has(ev)) betaOnlyEvents.add(ev);
  }
  for (const ev of betaAfter) {
    if (!stableAfterSet.has(ev)) betaOnlyEvents.add(ev);
  }

  // 2. Read-only events (from both stable and beta signal metadata)
  const readOnlyEvents = new Set([
    ...extractReadOnlyEvents(meta.stableBindings),
    ...extractReadOnlyEvents(meta.betaBindings),
  ]);

  // 3. Class-qualified Beta-only APIs (`ClassName.memberName`)
  const stableClasses = extractClassMembers(meta.stableBindings);
  const betaClasses = extractClassMembers(meta.betaBindings);
  const betaApis = [];
  for (const [className, members] of betaClasses.entries()) {
    if (className === "WorldBeforeEvents" || className === "WorldAfterEvents") {
      continue;
    }
    const stableMembers = stableClasses.get(className);
    for (const member of members) {
      if (!stableMembers || !stableMembers.has(member)) {
        betaApis.push(`${className}.${member}`);
      }
    }
  }

  // 4. Official vanilla IDs (merged cleanly with existing hand-curated lists)
  const vanillaItems = meta.vanillaItemsJson
    ? normalizeVanillaIds(meta.vanillaItemsJson)
    : existing.vanilla_items || [];
  const vanillaBlocks = meta.vanillaBlocksJson
    ? normalizeVanillaIds(meta.vanillaBlocksJson)
    : existing.vanilla_blocks || [];
  const syncedEffects = meta.vanillaEffectsJson
    ? normalizeVanillaIds(meta.vanillaEffectsJson)
    : [];
  const mergedEffects = sortedUnique([...(existing.effects || []), ...syncedEffects]);

  // Optional npm dist-tags cross-check
  let npmNote = "";
  if (args.checkNpm && !args.offlineDir) {
    try {
      const npmMeta = await fetchJson(NPM_SERVER_META);
      const tags = npmMeta["dist-tags"] || {};
      npmNote = ` | npm dist-tags: latest=${tags.latest || "?"}, beta=${tags.beta || "?"}`;
    } catch (err) {
      npmNote = ` | npm cross-check skipped (${err.message})`;
    }
  }

  const updated = {
    bedrock_version: existing.bedrock_version || "26.60",
    script_api_version:
      !args.offlineDir && meta.stableVersion
        ? meta.stableVersion
        : existing.script_api_version || "2.11.0",
    min_engine_version: existing.min_engine_version || [1, 26, 60],
    item_format_version: existing.item_format_version || "1.21.90",
    block_format_version: existing.block_format_version || "1.21.90",
    stable_before_events: sortedUnique(stableBefore),
    stable_after_events: sortedUnique(stableAfter),
    beta_events: sortedUnique(betaOnlyEvents),
    read_only_events: sortedUnique(readOnlyEvents),
    beta_apis: sortedUnique(betaApis),
    effects: mergedEffects,
    sounds: existing.sounds || [],
    item_textures: existing.item_textures || [],
    terrain_textures: existing.terrain_textures || [],
    vanilla_items: sortedUnique(vanillaItems),
    vanilla_blocks: sortedUnique(vanillaBlocks),
  };

  fs.mkdirSync(path.dirname(args.outPath), { recursive: true });
  fs.writeFileSync(args.outPath, `${JSON.stringify(updated, null, 2)}\n`, "utf8");

  console.log(
    [
      `Synced ${path.relative(REPO_ROOT, args.outPath) || args.outPath} (stable=${meta.stableVersion}, beta=${meta.betaVersion}${npmNote})`,
      `  stable_before_events : ${updated.stable_before_events.length}`,
      `  stable_after_events  : ${updated.stable_after_events.length}`,
      `  beta_events          : ${updated.beta_events.length} (${updated.beta_events.join(", ")})`,
      `  read_only_events     : ${updated.read_only_events.length}`,
      `  beta_apis            : ${updated.beta_apis.length}`,
      `  effects              : ${updated.effects.length}`,
      `  vanilla_items        : ${updated.vanilla_items.length}`,
      `  vanilla_blocks       : ${updated.vanilla_blocks.length}`,
    ].join("\n")
  );
}

main().catch((err) => {
  console.error(`sync_bedrock_spec failed: ${err.message}`);
  process.exit(1);
});
