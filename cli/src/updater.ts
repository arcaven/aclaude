/**
 * Self-update mechanism following Claude Code's version-directory + symlink pattern.
 *
 * Version storage: ~/.local/share/aclaude/versions/<version>/aclaude
 * Active symlink:  ~/.local/bin/aclaude (or aclaude-a for alpha)
 *
 * Channel and version are determined at build time via gen-version.ts.
 * No runtime detection — the binary knows what it is.
 */

import { execSync } from "node:child_process";
import { existsSync, mkdirSync, symlinkSync, unlinkSync, readdirSync, rmSync, renameSync, chmodSync } from "node:fs";
import { homedir, platform, arch } from "node:os";
import { join } from "node:path";
import { Writable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { VERSION, CHANNEL, COMMIT, BUILD_TIME } from "./version.js";

export type Channel = "stable" | "alpha";

export interface VersionInfo {
  version: string;
  tag: string;
  channel: Channel;
  publishedAt: string;
}

export interface UpdateResult {
  current: string;
  latest: string;
  updated: boolean;
  channel: Channel;
}

const GITHUB_OWNER = "arcaven";
const GITHUB_REPO = "aclaude";
const RELEASES_API = `https://api.github.com/repos/${GITHUB_OWNER}/${GITHUB_REPO}/releases`;

/**
 * Get the binary name for a channel.
 */
export function getBinaryName(channel: Channel): string {
  return channel === "alpha" ? "aclaude-a" : "aclaude";
}

/**
 * Get the platform-specific asset name for GitHub releases.
 * Format: aclaude-<platform>-<arch> or aclaude-a-<platform>-<arch>
 */
export function getAssetName(channel: Channel): string {
  const prefix = getBinaryName(channel);
  const p = platform();
  const a = arch();

  const platformName = p === "darwin" ? "darwin" : p === "linux" ? "linux" : p;
  const archName = a === "arm64" ? "arm64" : a === "x64" ? "amd64" : a;

  return `${prefix}-${platformName}-${archName}`;
}

/**
 * Paths for version management.
 */
export function getVersionsDir(): string {
  return join(homedir(), ".local", "share", "aclaude", "versions");
}

export function getSymlinkDir(): string {
  return join(homedir(), ".local", "bin");
}

export function getSymlinkPath(channel: Channel): string {
  return join(getSymlinkDir(), getBinaryName(channel));
}

export function getVersionDir(version: string): string {
  return join(getVersionsDir(), version);
}

/**
 * Get the current version — baked in at build time via gen-version.ts.
 */
export function getCurrentVersion(): string {
  return VERSION;
}

/**
 * Check GitHub releases API for the latest version on this channel.
 */
export async function checkForUpdate(channel: Channel): Promise<VersionInfo | null> {
  try {
    const response = await fetch(RELEASES_API, {
      headers: {
        "Accept": "application/vnd.github+json",
        "User-Agent": "aclaude-updater",
      },
    });

    if (!response.ok) {
      console.error(`Failed to check for updates: ${response.status} ${response.statusText}`);
      return null;
    }

    const releases = await response.json() as Array<{
      tag_name: string;
      prerelease: boolean;
      published_at: string;
      assets: Array<{ name: string; browser_download_url: string }>;
    }>;

    // Filter by channel: stable = non-prerelease with v* tag, alpha = prerelease with alpha-* tag
    const matching = releases.filter((r) => {
      if (channel === "stable") {
        return !r.prerelease && r.tag_name.startsWith("v");
      }
      return r.prerelease && r.tag_name.startsWith("alpha-");
    });

    if (matching.length === 0) {
      return null;
    }

    const latest = matching[0];
    const version = channel === "stable"
      ? latest.tag_name.replace(/^v/, "")
      : latest.tag_name;

    return {
      version,
      tag: latest.tag_name,
      channel,
      publishedAt: latest.published_at,
    };
  } catch (err) {
    console.error(`Failed to check for updates: ${err}`);
    return null;
  }
}

/**
 * Download a version from GitHub releases.
 */
export async function downloadVersion(info: VersionInfo): Promise<string> {
  const assetName = getAssetName(info.channel);
  const url = `https://github.com/${GITHUB_OWNER}/${GITHUB_REPO}/releases/download/${info.tag}/${assetName}`;

  console.log(`Downloading ${assetName} from ${info.tag}...`);

  const response = await fetch(url, {
    headers: { "User-Agent": "aclaude-updater" },
    redirect: "follow",
  });

  if (!response.ok) {
    throw new Error(`Download failed: ${response.status} ${response.statusText}`);
  }

  const versionDir = getVersionDir(info.version);
  mkdirSync(versionDir, { recursive: true });

  const binaryName = getBinaryName(info.channel);
  const binaryPath = join(versionDir, binaryName);

  // Stream the download to disk
  const fileHandle = await import("node:fs").then((fs) =>
    fs.createWriteStream(binaryPath)
  );

  if (!response.body) {
    throw new Error("No response body");
  }

  await pipeline(
    response.body as unknown as NodeJS.ReadableStream,
    fileHandle as unknown as Writable
  );

  // Make executable
  chmodSync(binaryPath, 0o755);

  console.log(`Downloaded to ${binaryPath}`);
  return binaryPath;
}

/**
 * Activate a version by rotating the symlink.
 */
export function activateVersion(version: string, channel: Channel): void {
  const binaryName = getBinaryName(channel);
  const versionBinary = join(getVersionDir(version), binaryName);

  if (!existsSync(versionBinary)) {
    throw new Error(`Version binary not found: ${versionBinary}`);
  }

  const symlinkDir = getSymlinkDir();
  mkdirSync(symlinkDir, { recursive: true });

  const symlinkPath = getSymlinkPath(channel);

  // Atomic symlink rotation: create temp link then rename
  const tmpLink = `${symlinkPath}.tmp`;
  try {
    if (existsSync(tmpLink)) unlinkSync(tmpLink);
    symlinkSync(versionBinary, tmpLink);
    renameSync(tmpLink, symlinkPath);
  } catch {
    // Fallback: direct replacement
    if (existsSync(symlinkPath)) unlinkSync(symlinkPath);
    symlinkSync(versionBinary, symlinkPath);
  }

  console.log(`Activated ${binaryName} ${version}`);
}

/**
 * List installed versions.
 */
export function listVersions(): string[] {
  const versionsDir = getVersionsDir();
  if (!existsSync(versionsDir)) return [];

  return readdirSync(versionsDir)
    .filter((d) => existsSync(join(versionsDir, d)))
    .sort()
    .reverse();
}

/**
 * Clean old versions, keeping the last N.
 */
export function cleanOldVersions(keep: number = 3): string[] {
  const versions = listVersions();
  const removed: string[] = [];

  if (versions.length <= keep) return removed;

  const toRemove = versions.slice(keep);
  for (const version of toRemove) {
    const dir = getVersionDir(version);
    rmSync(dir, { recursive: true, force: true });
    removed.push(version);
  }

  return removed;
}

/**
 * Check if ~/.local/bin is in PATH.
 */
export function isSymlinkDirInPath(): boolean {
  const symlinkDir = getSymlinkDir();
  const pathDirs = (process.env.PATH || "").split(":");
  return pathDirs.some((d) => d === symlinkDir);
}

/**
 * Run the update flow.
 */
export async function runUpdate(): Promise<UpdateResult> {
  const channel = CHANNEL;
  const current = getCurrentVersion();
  const binaryName = getBinaryName(channel);

  console.log(`${binaryName} ${current} (${channel} channel)`);
  console.log(`Built: ${BUILD_TIME} (${COMMIT})`);
  console.log("Checking for updates...");

  const latest = await checkForUpdate(channel);

  if (!latest) {
    console.log("No updates available (or unable to check).");
    return { current, latest: current, updated: false, channel };
  }

  if (latest.version === current) {
    console.log("Already up to date.");
    return { current, latest: current, updated: false, channel };
  }

  console.log(`New version available: ${latest.version} (published ${latest.publishedAt})`);

  await downloadVersion(latest);
  activateVersion(latest.version, channel);

  const removed = cleanOldVersions();
  if (removed.length > 0) {
    console.log(`Cleaned up ${removed.length} old version(s).`);
  }

  console.log(`\nUpdated ${binaryName}: ${current} → ${latest.version}`);
  return { current, latest: latest.version, updated: true, channel };
}

/**
 * Run first-time install (setup directories, activate current binary).
 */
export function runInstall(): void {
  const channel = CHANNEL;
  const version = getCurrentVersion();
  const binaryName = getBinaryName(channel);

  console.log(`Installing ${binaryName} ${version}...`);

  const versionDir = getVersionDir(version);
  mkdirSync(versionDir, { recursive: true });

  // Find the currently-running binary on disk.
  // In bun compile, process.argv[0] is a virtual FS path — useless.
  // Try: command -v <binaryName>, then command -v for the other name,
  // then resolve the brew symlink if applicable.
  let currentBinary: string | undefined;
  for (const name of [binaryName, "aclaude", "aclaude-a"]) {
    try {
      const found = execSync(`command -v ${name}`, { encoding: "utf-8", shell: "/bin/sh" }).trim();
      if (found && existsSync(found)) {
        currentBinary = found;
        break;
      }
    } catch {
      // not found, try next
    }
  }

  const targetBinary = join(versionDir, binaryName);

  if (currentBinary && existsSync(currentBinary)) {
    try {
      execSync(`cp "${currentBinary}" "${targetBinary}"`);
      chmodSync(targetBinary, 0o755);
    } catch (err) {
      console.error(`Failed to copy binary: ${err}`);
      console.log("You can manually copy the binary to:", targetBinary);
      return;
    }
  } else {
    console.error("Could not locate the current binary to copy.");
    console.log("You can manually copy the binary to:", targetBinary);
    return;
  }

  activateVersion(version, channel);

  if (!isSymlinkDirInPath()) {
    console.log(`\nAdd ${getSymlinkDir()} to your PATH:`);
    console.log(`  echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc`);
  }

  console.log(`\n${binaryName} ${version} installed.`);
}

/**
 * Show version info with optional update check.
 */
export async function showVersion(check: boolean = false): Promise<void> {
  const channel = CHANNEL;
  const current = getCurrentVersion();
  const binaryName = getBinaryName(channel);

  console.log(`${binaryName} ${current} (${channel} channel)`);

  if (check) {
    const latest = await checkForUpdate(channel);
    if (latest && latest.version !== current) {
      console.log(`Update available: ${latest.version} (run '${binaryName} update')`);
    } else if (latest) {
      console.log("Up to date.");
    }
  }
}
