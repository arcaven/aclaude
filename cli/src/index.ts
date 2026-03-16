#!/usr/bin/env node

import { Command } from "commander";
import { loadConfig, getConfigPaths } from "./config.js";
import { listThemes, loadTheme, getAgent } from "./persona.js";
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
    if (opts.theme || opts.role || opts.immersion) {
      overrides.persona = {
        ...(opts.theme && { theme: opts.theme }),
        ...(opts.role && { role: opts.role }),
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
  .action((name: string) => {
    const theme = loadTheme(name);
    if (!theme) {
      console.error(`Theme "${name}" not found.`);
      process.exit(1);
    }

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
      console.log(`  ${role.padEnd(15)} ${agent.character}`);
      console.log(`  ${"".padEnd(15)} ${agent.style}`);
    }
  });

program.parse();
