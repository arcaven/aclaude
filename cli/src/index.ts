#!/usr/bin/env node

import { Command } from "commander";
import { loadConfig, getConfigPaths } from "./config.js";
import { listThemes, loadTheme, getAgent, resolvePortrait, getPortraitCachePath, displayPortrait, terminalSupportsImages } from "./persona.js";
import { startSession } from "./session.js";
import { initTelemetry } from "./telemetry.js";

const program = new Command();

program
  .name("aclaude")
  .description("BOYC agent orchestration CLI")
  .version("0.1.0");

// Default command — start interactive session
program
  .option("-m, --model <model>", "model to use")
  .option("-t, --theme <theme>", "persona theme")
  .option("-r, --role <role>", "persona role (character from theme)")
  .option("-i, --immersion <level>", "immersion level: high|medium|low|none")
  .action(async (opts) => {
    const overrides: Record<string, unknown> = {};
    if (opts.model) overrides.session = { model: opts.model };
    if (opts.theme || opts.agent || opts.immersion) {
      overrides.persona = {
        ...(opts.theme && { theme: opts.theme }),
        ...(opts.agent && { role: opts.agent }),
        ...(opts.immersion && { immersion: opts.immersion }),
      };
    }

    const config = loadConfig(overrides as never);
    initTelemetry(config.telemetry);

    const theme = loadTheme(config.persona.theme);
    const agent = theme ? getAgent(theme, config.persona.role) : null;

    if (!theme) {
      console.warn(`Warning: theme "${config.persona.theme}" not found. Starting without persona.`);
    } else if (!agent) {
      console.warn(`Warning: role "${config.persona.role}" not found in theme "${config.persona.theme}". Starting without persona.`);
    }

    await startSession(config, theme, agent);
  });

// config command
const configCmd = program.command("config").description("Show resolved configuration");

configCmd
  .action(() => {
    const config = loadConfig();
    console.log(JSON.stringify(config, null, 2));
  });

configCmd
  .command("path")
  .description("Print config file locations")
  .action(() => {
    const paths = getConfigPaths();
    console.log(`Defaults:  ${paths.defaults}`);
    console.log(`Global:    ${paths.global}`);
    console.log(`Local:     ${paths.local}`);
  });

// persona commands
const personaCmd = program.command("persona").description("Manage personas");

personaCmd
  .command("list")
  .description("List available themes")
  .action(() => {
    const themes = listThemes();
    if (themes.length === 0) {
      console.log("No themes found.");
      return;
    }
    console.log(`${themes.length} themes available:\n`);
    for (const slug of themes) {
      const theme = loadTheme(slug);
      if (theme) {
        const roles = Object.keys(theme.agents).join(", ");
        console.log(`  ${slug.padEnd(30)} ${theme.name}`);
        console.log(`  ${"".padEnd(30)} roles: ${roles}`);
      } else {
        console.log(`  ${slug}`);
      }
    }
  });

personaCmd
  .command("show <name>")
  .description("Show theme details")
  .option("-p, --portrait", "display portraits inline (Kitty/Ghostty)")
  .option("--portrait-position <pos>", "portrait position: top|bottom|left|right", "top")
  .option("--agent <role>", "show only this agent/role (with portrait if -p)")
  .action((name: string, opts: { portrait?: boolean; portraitPosition?: string; agent?: string }) => {
    const theme = loadTheme(name);
    if (!theme) {
      console.error(`Theme "${name}" not found.`);
      process.exit(1);
    }

    // If --role specified, show just that agent
    if (opts.agent) {
      const agent = getAgent(theme, opts.agent);
      if (!agent) {
        console.error(`Role "${opts.agent}" not found in theme "${name}".`);
        console.error(`Available: ${Object.keys(theme.agents).join(", ")}`);
        process.exit(1);
      }
      const portrait = resolvePortrait(name, agent, opts.agent);
      const imgPath = portrait.large || portrait.medium || portrait.small || null;
      const position = (opts.portraitPosition || "top") as "top" | "bottom" | "left" | "right";
      const showImage = opts.portrait && imgPath;

      // Portrait before card (top or left)
      if (showImage && (position === "top" || position === "left")) {
        if (!displayPortrait(imgPath!, { position })) {
          console.log("(terminal does not support inline images — try Kitty or Ghostty)");
        }
        console.log("");
      }

      console.log(`Theme: ${theme.name}`);
      console.log(`Role:  ${opts.agent}`);
      console.log(`Character: ${agent.character}`);
      console.log(`Style: ${agent.style}`);
      console.log(`Expertise: ${agent.expertise}`);
      if (agent.trait) console.log(`Trait: ${agent.trait}`);
      if (agent.quirks?.length) console.log(`Quirks: ${agent.quirks.join("; ")}`);
      if (agent.catchphrases?.length) {
        console.log(`Catchphrases:`);
        for (const c of agent.catchphrases) console.log(`  "${c}"`);
      }
      if (imgPath) {
        const stem = imgPath.split("/").pop()?.replace(/\.png$/, "") || "";
        console.log(`Portrait: ${stem}.png`);
      } else {
        console.log("Portrait: not installed");
      }

      // Portrait after card (bottom or right)
      if (showImage && (position === "bottom" || position === "right")) {
        console.log("");
        if (!displayPortrait(imgPath!, { position })) {
          console.log("(terminal does not support inline images — try Kitty or Ghostty)");
        }
      }
      return;
    }

    // Full theme display
    console.log(`Theme: ${theme.name}`);
    console.log(`Slug:  ${theme.slug}`);
    console.log(`Source: ${theme.source}`);
    console.log(`Description: ${theme.description}`);
    if (theme.user_title) console.log(`User title: ${theme.user_title}`);
    if (theme.dimensions) {
      console.log(`\nDimensions:`);
      for (const [key, val] of Object.entries(theme.dimensions)) {
        console.log(`  ${key}: ${val}`);
      }
    }
    if (theme.spinner_verbs?.length) {
      console.log(`\nSpinner verbs: ${theme.spinner_verbs.slice(0, 5).join(", ")}...`);
    }
    console.log(`\nAgents (${Object.keys(theme.agents).length}):`);
    for (const [role, agent] of Object.entries(theme.agents)) {
      const portrait = resolvePortrait(name, agent, role);
      const sizes = Object.keys(portrait);
      const portraitStatus = sizes.length > 0
        ? `[${sizes.join(",")}]`
        : "[no portraits]";
      console.log(`  ${role.padEnd(15)} ${agent.character}`);
      console.log(`  ${"".padEnd(15)} ${agent.style}`);
      console.log(`  ${"".padEnd(15)} portraits: ${portraitStatus}`);
      if (portrait.small) {
        const stem = portrait.small.split("/").pop()?.replace(/\.png$/, "") || "";
        console.log(`  ${"".padEnd(15)} file: ${stem}.png`);
      }
    }
  });

personaCmd
  .command("portraits")
  .description("Show portrait cache status")
  .action(async () => {
    const { existsSync, readdirSync } = await import("node:fs");
    const cachePath = getPortraitCachePath();
    console.log(`Portrait cache: ${cachePath}`);

    if (!existsSync(cachePath)) {
      console.log("Status: not installed");
      console.log("\nRun 'aclaude sync' to populate portraits from sources.toml");
      return;
    }

    const themeDirs = readdirSync(cachePath).filter((d) =>
      existsSync(`${cachePath}/${d}`) && !d.startsWith(".")
    );
    let totalImages = 0;
    for (const theme of themeDirs) {
      for (const size of ["small", "medium", "large", "original"]) {
        const sizeDir = `${cachePath}/${theme}/${size}`;
        if (existsSync(sizeDir)) {
          totalImages += readdirSync(sizeDir).filter((f) => f.endsWith(".png")).length;
        }
      }
    }
    console.log(`Themes with portraits: ${themeDirs.length}`);
    console.log(`Total images: ${totalImages}`);
  });

program.parse();
