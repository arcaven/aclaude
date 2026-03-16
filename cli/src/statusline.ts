import { execSync } from "node:child_process";
import { writeFileSync, mkdirSync, existsSync } from "node:fs";
import { join } from "node:path";
import type { AclaudeConfig } from "./config.js";

// ANSI colors
const RESET = "\x1b[0m";
const DIM = "\x1b[2m";
const BOLD = "\x1b[1m";
const FG_CYAN = "\x1b[36m";
const FG_GREEN = "\x1b[32m";
const FG_YELLOW = "\x1b[33m";
const FG_RED = "\x1b[31m";
const FG_GRAY = "\x1b[38;5;245m";
const FG_TEAL = "\x1b[38;5;43m";

interface GitInfo {
  branch: string;
  dirty: boolean;
}

function getGitInfo(cwd: string): GitInfo {
  try {
    let branch = execSync("git branch --show-current", { cwd, timeout: 3000, encoding: "utf-8" }).trim();
    if (!branch) {
      branch = execSync("git rev-parse --short HEAD", { cwd, timeout: 3000, encoding: "utf-8" }).trim();
    }
    let dirty = false;
    try {
      execSync("git diff-index --quiet HEAD --", { cwd, timeout: 3000 });
    } catch {
      dirty = true;
    }
    if (!dirty) {
      const untracked = execSync("git ls-files --others --exclude-standard", { cwd, timeout: 3000, encoding: "utf-8" }).trim();
      if (untracked) dirty = true;
    }
    return { branch, dirty };
  } catch {
    return { branch: "", dirty: false };
  }
}

function buildProgressBar(pct: number | null): { bar: string; display: string } {
  const width = 10;
  if (pct === null) {
    return {
      bar: `${DIM}${"░".repeat(width)}${RESET}`,
      display: `${FG_GRAY}--%${RESET}`,
    };
  }

  const filled = Math.max(0, Math.min(width, Math.floor((pct * width) / 100)));
  let color: string;
  if (pct > 95) color = `${FG_RED}${BOLD}`;
  else if (pct > 85) color = FG_RED;
  else if (pct > 70) color = FG_YELLOW;
  else color = FG_GREEN;

  return {
    bar: `${color}${"▓".repeat(filled)}${RESET}${DIM}${"░".repeat(width - filled)}${RESET}`,
    display: `${color}${pct}%${RESET}`,
  };
}

// tmux-formatted context bar
function tmuxContextBar(pct: number | null): string {
  const width = 10;
  if (pct === null) {
    return `#[fg=colour240]${"░".repeat(width)} --%#[default]`;
  }

  const filled = Math.max(0, Math.min(width, Math.floor((pct * width) / 100)));
  let color: string;
  if (pct > 95) color = "fg=colour196,bold";
  else if (pct > 85) color = "fg=colour196";
  else if (pct > 70) color = "fg=colour214";
  else color = "fg=colour34";

  const bar = `#[${color}]${"▓".repeat(filled)}#[fg=colour240]${"░".repeat(width - filled)}#[default]`;
  return `${bar} #[${color}]${pct}%#[default]`;
}

export function renderStatusline(config: AclaudeConfig, opts: { characterName?: string; contextPct?: number | null }): string {
  if (!config.statusline.enabled) return "";

  const cwd = process.cwd();
  const dirName = cwd.split("/").pop() || "?";
  const model = config.session.model.replace(/^claude-/, "").replace(/-\d+$/, "").slice(0, 10);

  let branch = "";
  let dirty = false;
  if (config.statusline.git_info) {
    const git = getGitInfo(cwd);
    branch = git.branch;
    dirty = git.dirty;
  }

  const pct = opts.contextPct ?? null;
  const { bar, display } = buildProgressBar(pct);

  const branchColor = dirty ? FG_YELLOW : FG_GREEN;
  const branchStr = `${branch}${dirty ? "*" : ""}`.slice(0, 12).padEnd(12);

  const charSection = opts.characterName ? `${DIM}${opts.characterName}${RESET} ` : "";

  // OTEL indicator
  let otelSuffix = "";
  if (config.telemetry.enabled && config.telemetry.otel_endpoint) {
    const portMatch = config.telemetry.otel_endpoint.match(/:(\d+)$/);
    if (portMatch) {
      otelSuffix = ` ${DIM}│${RESET} ${FG_TEAL}otel:${portMatch[1]}${RESET}`;
    }
  }

  return (
    `${charSection}` +
    `${DIM}│${RESET} ` +
    `${FG_CYAN}${dirName.padEnd(14)}${RESET}` +
    `${DIM}│${RESET} ` +
    `${branchColor}${branchStr}${RESET}` +
    `${DIM}│${RESET} ` +
    `${FG_GRAY}${model.padEnd(10)}${RESET}` +
    `${bar} ${display}` +
    `${otelSuffix}`
  );
}

export function writeTmuxCache(config: AclaudeConfig, opts: { contextPct?: number | null }): void {
  const cacheDir = join(process.cwd(), ".aclaude");
  if (!existsSync(cacheDir)) {
    mkdirSync(cacheDir, { recursive: true });
  }

  const dirName = process.cwd().split("/").pop() || "?";
  const pct = opts.contextPct ?? null;
  const sep = " #[fg=colour238]│#[default] ";

  try {
    const left = `#[fg=colour67]${dirName}#[default]`;
    writeFileSync(join(cacheDir, "tmux-status-left"), left);
    writeFileSync(join(cacheDir, "tmux-status-right"), tmuxContextBar(pct));
  } catch {
    // Ignore write errors
  }
}
