import { query } from "@anthropic-ai/claude-agent-sdk";
import type { SDKMessage } from "@anthropic-ai/claude-agent-sdk/entrypoints/sdk/coreTypes.js";
import { createInterface } from "node:readline";
import { execSync } from "node:child_process";
import type { AclaudeConfig } from "./config.js";
import type { PersonaAgent, PersonaTheme } from "./persona.js";
import { buildSystemPrompt } from "./persona.js";
import { SessionHooks } from "./hooks.js";
import { renderStatusline, writeTmuxCache } from "./statusline.js";
import { traceSpanAsync } from "./telemetry.js";

interface SessionUsage {
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  contextWindowSize: number;
  costUsd: number;
  numTurns: number;
}

function extractUsageFromResult(message: Record<string, unknown>): Partial<SessionUsage> {
  const result: Partial<SessionUsage> = {};

  // Aggregate usage
  const usage = message.usage as Record<string, number> | undefined;
  if (usage) {
    result.inputTokens = usage.input_tokens || 0;
    result.outputTokens = usage.output_tokens || 0;
    result.cacheReadTokens = usage.cache_read_input_tokens || 0;
    result.cacheCreationTokens = usage.cache_creation_input_tokens || 0;
  }

  // Per-model usage (get context window size from first model)
  const modelUsage = message.modelUsage as Record<string, Record<string, number>> | undefined;
  if (modelUsage) {
    for (const mu of Object.values(modelUsage)) {
      if (mu.contextWindow) {
        result.contextWindowSize = mu.contextWindow;
        break;
      }
    }
  }

  if (typeof message.total_cost_usd === "number") {
    result.costUsd = message.total_cost_usd;
  }
  if (typeof message.num_turns === "number") {
    result.numTurns = message.num_turns;
  }

  return result;
}

function computeContextPct(usage: Partial<SessionUsage>): number | null {
  // Input tokens = what's in the context window this turn.
  // Cache read/creation tokens are billing details, not context occupancy.
  const used = usage.inputTokens || 0;
  const size = usage.contextWindowSize || 0;
  if (!used || !size) return null;
  return Math.min(100, Math.floor((used / size) * 100));
}

export async function startSession(
  config: AclaudeConfig,
  theme: PersonaTheme | null,
  agent: PersonaAgent | null,
): Promise<void> {
  // Use Claude Code's default system prompt as the base, append persona on top.
  // This preserves all built-in tool instructions, safety guidelines, and capabilities.
  const personaPrompt = theme && agent
    ? buildSystemPrompt(theme, agent, config.persona.immersion)
    : "";

  const systemPrompt: string | { type: "preset"; preset: "claude_code"; append?: string } = personaPrompt
    ? { type: "preset", preset: "claude_code", append: personaPrompt }
    : { type: "preset", preset: "claude_code" };

  const characterName = agent?.shortName || agent?.character || undefined;

  // Set up hooks
  const hooks = new SessionHooks({
    onToolUse: (record) => {
      if (record.success === false) {
        console.error(`  [tool failed] ${record.tool_name}: ${record.input_summary}`);
      }
    },
  });

  let sessionId: string | undefined;
  let cumulativeUsage: Partial<SessionUsage> = {};
  let lastKnownPct: number | null = null;

  // Auth info: note when using Claude Code's inherited auth
  const hasApiKey = !!(process.env.ANTHROPIC_API_KEY ||
    process.env.AWS_ACCESS_KEY_ID ||  // Bedrock
    process.env.GOOGLE_APPLICATION_CREDENTIALS);  // Vertex AI

  if (!hasApiKey) {
    console.log("Auth: using Claude Code credentials (single-user).");
    console.log("");
  }

  // Locate claude executable — required when running from a compiled binary
  // where the SDK can't resolve it from import.meta.url
  let claudePath: string | undefined;
  try {
    claudePath = execSync("command -v claude", { encoding: "utf-8", shell: "/bin/sh" }).trim();
  } catch {
    // not found
  }
  if (!claudePath) {
    console.error("Error: Claude Code CLI (claude) not found in PATH.");
    console.error("Install it from: https://docs.anthropic.com/en/docs/claude-code");
    process.exit(1);
  }

  console.log("Starting session (via Claude Code)...");
  if (characterName) {
    console.log(`Persona: ${agent!.character} (${theme!.name})`);
  }
  console.log(`Model: ${config.session.model}`);
  console.log("");

  const rl = createInterface({ input: process.stdin, output: process.stdout });
  const prompt = (): Promise<string> =>
    new Promise((resolve) => {
      rl.question("> ", (answer) => resolve(answer));
    });

  // Initial statusline
  const statusline = renderStatusline(config, { characterName, contextPct: null });
  if (statusline) console.log(statusline);
  writeTmuxCache(config, { contextPct: null });

  try {
    while (true) {
      const input = await prompt();
      if (!input.trim()) continue;
      if (input.trim() === "/quit" || input.trim() === "/exit") break;

      if (input.trim() === "/usage") {
        printUsageSummary(cumulativeUsage, hooks);
        continue;
      }

      if (input.trim() === "/help") {
        console.log("aclaude commands:");
        console.log("  /usage    — show session token usage and tool stats");
        console.log("  /quit     — end session");
        console.log("");
        console.log("All Claude Code slash commands (e.g. /compact, /clear, /model)");
        console.log("are passed through to Claude Code.");
        continue;
      }

      try {
      await traceSpanAsync("message", async () => {
        process.stdout.write("\n");

        const q = query({
          prompt: input,
          options: {
            systemPrompt,
            model: config.session.model,
            ...(sessionId && { resume: sessionId }),
            ...(claudePath && { pathToClaudeCodeExecutable: claudePath }),
            settingSources: ["project", "user", "local"],
            includePartialMessages: true,
            permissionMode: "default",
            hooks: hooks.buildHooksConfig(),
          },
        });

        for await (const message of q) {
          // Capture session ID from init
          if (message.type === "system" && "subtype" in message && (message as Record<string, unknown>).subtype === "init") {
            sessionId = (message as Record<string, unknown>).session_id as string;
          }

          // Stream text deltas
          if (message.type === "stream_event") {
            const event = (message as Record<string, unknown>).event as Record<string, unknown>;
            if (event?.type === "content_block_delta") {
              const delta = event.delta as Record<string, unknown>;
              if (delta?.type === "text_delta" && typeof delta.text === "string") {
                process.stdout.write(delta.text);
              }
            }
          }

          // Per-turn usage from assistant messages
          if (message.type === "assistant") {
            const assistantMsg = message as Record<string, unknown>;
            const betaMessage = assistantMsg.message as Record<string, unknown> | undefined;
            if (betaMessage?.usage) {
              const usage = betaMessage.usage as Record<string, number>;
              cumulativeUsage.inputTokens = usage.input_tokens || 0;
              cumulativeUsage.outputTokens = (cumulativeUsage.outputTokens || 0) + (usage.output_tokens || 0);
              if (usage.cache_read_input_tokens) {
                cumulativeUsage.cacheReadTokens = usage.cache_read_input_tokens;
              }
              if (usage.cache_creation_input_tokens) {
                cumulativeUsage.cacheCreationTokens = usage.cache_creation_input_tokens;
              }
            }
          }

          // Final result — authoritative usage
          if ("result" in message) {
            const resultUsage = extractUsageFromResult(message as Record<string, unknown>);
            cumulativeUsage = { ...cumulativeUsage, ...resultUsage };
          }
        }

        process.stdout.write("\n\n");

        // Update statusline — preserve last known context pct
        const pct = computeContextPct(cumulativeUsage);
        if (pct !== null) lastKnownPct = pct;
        const displayPct = pct ?? lastKnownPct;
        const sl = renderStatusline(config, { characterName, contextPct: displayPct });
        if (sl) console.log(sl);
        writeTmuxCache(config, { contextPct: displayPct });
      });
      } catch (err) {
        process.stdout.write("\n");
        if (err instanceof ReferenceError && String(err).includes("Claude Code executable")) {
          console.error(`Error: ${err.message}`);
          break;
        }
        console.error(`Error: ${err instanceof Error ? err.message : String(err)}`);
        // Show statusline even after errors
        const sl = renderStatusline(config, { characterName, contextPct: lastKnownPct });
        if (sl) console.log(sl);
      }
    }
  } finally {
    rl.close();
  }

  // Print session summary
  console.log("");
  printUsageSummary(cumulativeUsage, hooks);
  console.log("Session ended.");
}

function printUsageSummary(usage: Partial<SessionUsage>, hooks: SessionHooks): void {
  const pct = computeContextPct(usage);
  console.log("--- Session Usage ---");
  if (usage.inputTokens !== undefined) {
    console.log(`  Input tokens:    ${usage.inputTokens.toLocaleString()}`);
  }
  if (usage.outputTokens !== undefined) {
    console.log(`  Output tokens:   ${usage.outputTokens.toLocaleString()}`);
  }
  if (usage.cacheReadTokens) {
    console.log(`  Cache read:      ${usage.cacheReadTokens.toLocaleString()}`);
  }
  if (usage.cacheCreationTokens) {
    console.log(`  Cache creation:  ${usage.cacheCreationTokens.toLocaleString()}`);
  }
  if (pct !== null) {
    console.log(`  Context usage:   ${pct}%`);
  }
  if (usage.costUsd !== undefined) {
    console.log(`  Cost:            $${usage.costUsd.toFixed(4)}`);
  }
  if (usage.numTurns !== undefined) {
    console.log(`  Turns:           ${usage.numTurns}`);
  }

  const toolCounts = hooks.getToolCounts();
  if (Object.keys(toolCounts).length > 0) {
    console.log(`  Tools used:      ${Object.entries(toolCounts).map(([k, v]) => `${k}(${v})`).join(", ")}`);
  }

  const failed = hooks.getFailedTools();
  if (failed.length > 0) {
    console.log(`  Failed tools:    ${failed.length}`);
  }
  console.log("---");
}
