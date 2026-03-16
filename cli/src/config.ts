import { readFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { parse as parseToml } from "smol-toml";

export interface SessionConfig {
  model: string;
  max_tokens: number;
}

export interface PersonaConfig {
  theme: string;
  role: string;
  immersion: "high" | "medium" | "low" | "none";
}

export interface StatuslineConfig {
  enabled: boolean;
  git_info: boolean;
  context_bar: boolean;
}

export interface TelemetryConfig {
  enabled: boolean;
  otel_endpoint: string;
}

export interface TmuxConfig {
  layout: "bottom" | "top" | "left" | "right";
  socket: string;
}

export interface AclaudeConfig {
  session: SessionConfig;
  persona: PersonaConfig;
  statusline: StatuslineConfig;
  telemetry: TelemetryConfig;
  tmux: TmuxConfig;
}

function deepMerge(target: Record<string, unknown>, source: Record<string, unknown>): Record<string, unknown> {
  const result = { ...target };
  for (const key of Object.keys(source)) {
    const sv = source[key];
    const tv = target[key];
    if (sv !== null && typeof sv === "object" && !Array.isArray(sv) && tv !== null && typeof tv === "object" && !Array.isArray(tv)) {
      result[key] = deepMerge(tv as Record<string, unknown>, sv as Record<string, unknown>);
    } else {
      result[key] = sv;
    }
  }
  return result;
}

function loadToml(path: string): Record<string, unknown> {
  if (!existsSync(path)) return {};
  try {
    return parseToml(readFileSync(path, "utf-8")) as Record<string, unknown>;
  } catch {
    return {};
  }
}

function getXdgConfigHome(): string {
  return process.env.XDG_CONFIG_HOME || join(process.env.HOME || "~", ".config");
}

function applyEnvOverrides(config: Record<string, unknown>): Record<string, unknown> {
  const prefix = "ACLAUDE_";
  for (const [key, value] of Object.entries(process.env)) {
    if (!key.startsWith(prefix) || value === undefined) continue;
    const parts = key.slice(prefix.length).toLowerCase().split("__");
    if (parts.length === 2) {
      const [section, field] = parts;
      if (!config[section] || typeof config[section] !== "object") {
        config[section] = {};
      }
      const sectionObj = config[section] as Record<string, unknown>;
      // Parse booleans and numbers
      if (value === "true") sectionObj[field] = true;
      else if (value === "false") sectionObj[field] = false;
      else if (/^\d+$/.test(value)) sectionObj[field] = parseInt(value, 10);
      else sectionObj[field] = value;
    }
  }
  return config;
}

export function getConfigPaths(): { defaults: string; global: string; local: string } {
  const projectRoot = findProjectRoot();
  return {
    defaults: join(projectRoot, "config", "defaults.toml"),
    global: join(getXdgConfigHome(), "aclaude", "config.toml"),
    local: join(process.cwd(), ".aclaude", "config.toml"),
  };
}

function findProjectRoot(): string {
  // Walk up from this file to find the repo root (where config/ lives)
  let dir = new URL(".", import.meta.url).pathname;
  for (let i = 0; i < 10; i++) {
    if (existsSync(join(dir, "config", "defaults.toml"))) return dir;
    const parent = join(dir, "..");
    if (parent === dir) break;
    dir = parent;
  }
  return process.cwd();
}

export function loadConfig(overrides?: Partial<AclaudeConfig>): AclaudeConfig {
  const paths = getConfigPaths();

  // Layer 1: defaults
  let config = loadToml(paths.defaults);

  // Layer 2: global
  config = deepMerge(config, loadToml(paths.global));

  // Layer 3: local
  config = deepMerge(config, loadToml(paths.local));

  // Layer 4: env
  config = applyEnvOverrides(config);

  // Layer 5: CLI overrides
  if (overrides) {
    config = deepMerge(config, overrides as Record<string, unknown>);
  }

  return config as unknown as AclaudeConfig;
}
