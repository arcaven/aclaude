/**
 * Hook system — wires into Claude Agent SDK's hook callbacks to track
 * tool usage, context window, and session lifecycle. Replaces
 * pennyfarthing's shell-based hooks with in-process JS callbacks.
 */

import type {
  HookCallbackMatcher,
  HookCallback,
} from "@anthropic-ai/claude-agent-sdk";
import type {
  HookInput,
  HookJSONOutput,
  PreToolUseHookInput,
  PostToolUseHookInput,
  PostToolUseFailureHookInput,
  StopHookInput,
  SessionStartHookInput,
  SessionEndHookInput,
} from "@anthropic-ai/claude-agent-sdk/entrypoints/sdk/coreTypes.js";

// ---------------------------------------------------------------------------
// Event log — append-only audit trail for the session
// ---------------------------------------------------------------------------

export interface HookEvent {
  timestamp: string;
  event: string;
  session_id: string;
  data: Record<string, unknown>;
}

export interface ToolUsageRecord {
  tool_name: string;
  tool_use_id?: string;
  input_summary: string;
  timestamp: string;
  duration_ms?: number;
  success?: boolean;
}

export class SessionHooks {
  readonly events: HookEvent[] = [];
  readonly toolUsage: ToolUsageRecord[] = [];

  private _toolStartTimes = new Map<string, number>();
  private _onToolUse?: (record: ToolUsageRecord) => void;
  private _onEvent?: (event: HookEvent) => void;

  constructor(opts?: {
    onToolUse?: (record: ToolUsageRecord) => void;
    onEvent?: (event: HookEvent) => void;
  }) {
    this._onToolUse = opts?.onToolUse;
    this._onEvent = opts?.onEvent;
  }

  private _log(event: string, sessionId: string, data: Record<string, unknown>): void {
    const entry: HookEvent = {
      timestamp: new Date().toISOString(),
      event,
      session_id: sessionId,
      data,
    };
    this.events.push(entry);
    this._onEvent?.(entry);
  }

  private _summarizeInput(input: unknown): string {
    if (!input || typeof input !== "object") return "";
    const obj = input as Record<string, unknown>;
    // Prefer file_path, then command, then path, then first string value
    if (typeof obj.file_path === "string") return obj.file_path;
    if (typeof obj.command === "string") return obj.command.slice(0, 80);
    if (typeof obj.path === "string") return obj.path;
    if (typeof obj.pattern === "string") return obj.pattern;
    const first = Object.values(obj).find((v) => typeof v === "string");
    return typeof first === "string" ? first.slice(0, 80) : "";
  }

  // -------------------------------------------------------------------------
  // Hook callbacks — these match the HookCallback signature
  // -------------------------------------------------------------------------

  private _onPreToolUse: HookCallback = async (input, _toolUseId, _opts) => {
    const hook = input as PreToolUseHookInput;
    this._log("PreToolUse", hook.session_id, {
      tool_name: hook.tool_name,
      tool_use_id: hook.tool_use_id,
      input_summary: this._summarizeInput(hook.tool_input),
    });

    if (hook.tool_use_id) {
      this._toolStartTimes.set(hook.tool_use_id, Date.now());
    }

    return { continue: true };
  };

  private _onPostToolUse: HookCallback = async (input, _toolUseId, _opts) => {
    const hook = input as PostToolUseHookInput;
    const startTime = hook.tool_use_id ? this._toolStartTimes.get(hook.tool_use_id) : undefined;
    const durationMs = startTime ? Date.now() - startTime : undefined;

    if (hook.tool_use_id) {
      this._toolStartTimes.delete(hook.tool_use_id);
    }

    const record: ToolUsageRecord = {
      tool_name: hook.tool_name,
      tool_use_id: hook.tool_use_id,
      input_summary: this._summarizeInput(hook.tool_input),
      timestamp: new Date().toISOString(),
      duration_ms: durationMs,
      success: true,
    };
    this.toolUsage.push(record);
    this._onToolUse?.(record);

    this._log("PostToolUse", hook.session_id, {
      tool_name: hook.tool_name,
      tool_use_id: hook.tool_use_id,
      duration_ms: durationMs,
    });

    return { continue: true };
  };

  private _onPostToolUseFailure: HookCallback = async (input, _toolUseId, _opts) => {
    const hook = input as PostToolUseFailureHookInput;
    const startTime = hook.tool_use_id ? this._toolStartTimes.get(hook.tool_use_id) : undefined;
    const durationMs = startTime ? Date.now() - startTime : undefined;

    if (hook.tool_use_id) {
      this._toolStartTimes.delete(hook.tool_use_id);
    }

    const record: ToolUsageRecord = {
      tool_name: hook.tool_name,
      tool_use_id: hook.tool_use_id,
      input_summary: this._summarizeInput(hook.tool_input),
      timestamp: new Date().toISOString(),
      duration_ms: durationMs,
      success: false,
    };
    this.toolUsage.push(record);
    this._onToolUse?.(record);

    this._log("PostToolUseFailure", hook.session_id, {
      tool_name: hook.tool_name,
      tool_use_id: hook.tool_use_id,
      error: String((hook as Record<string, unknown>).error || ""),
    });

    return { continue: true };
  };

  private _onSessionStart: HookCallback = async (input, _toolUseId, _opts) => {
    const hook = input as SessionStartHookInput;
    this._log("SessionStart", hook.session_id, { source: hook.source });
    return { continue: true };
  };

  private _onStop: HookCallback = async (input, _toolUseId, _opts) => {
    const hook = input as StopHookInput;
    this._log("Stop", hook.session_id, { stop_hook_active: hook.stop_hook_active });
    return { continue: true };
  };

  private _onSessionEnd: HookCallback = async (input, _toolUseId, _opts) => {
    const hook = input as SessionEndHookInput;
    this._log("SessionEnd", hook.session_id, { reason: hook.reason });
    return { continue: true };
  };

  // -------------------------------------------------------------------------
  // Build the hooks config for query() options
  // -------------------------------------------------------------------------

  buildHooksConfig(): Partial<Record<string, HookCallbackMatcher[]>> {
    return {
      PreToolUse: [{ hooks: [this._onPreToolUse] }],
      PostToolUse: [{ hooks: [this._onPostToolUse] }],
      PostToolUseFailure: [{ hooks: [this._onPostToolUseFailure] }],
      SessionStart: [{ hooks: [this._onSessionStart] }],
      Stop: [{ hooks: [this._onStop] }],
      SessionEnd: [{ hooks: [this._onSessionEnd] }],
    };
  }

  // -------------------------------------------------------------------------
  // Accessors
  // -------------------------------------------------------------------------

  getToolCounts(): Record<string, number> {
    const counts: Record<string, number> = {};
    for (const record of this.toolUsage) {
      counts[record.tool_name] = (counts[record.tool_name] || 0) + 1;
    }
    return counts;
  }

  getFailedTools(): ToolUsageRecord[] {
    return this.toolUsage.filter((r) => r.success === false);
  }
}
