import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { loadConfig, getConfigPaths } from "../src/config.js";

describe("config", () => {
  const savedEnv: Record<string, string | undefined> = {};

  beforeEach(() => {
    // Save env vars we might modify
    for (const key of Object.keys(process.env)) {
      if (key.startsWith("ACLAUDE_")) {
        savedEnv[key] = process.env[key];
        delete process.env[key];
      }
    }
  });

  afterEach(() => {
    // Clean up ACLAUDE_ env vars
    for (const key of Object.keys(process.env)) {
      if (key.startsWith("ACLAUDE_")) {
        delete process.env[key];
      }
    }
    // Restore saved env vars
    for (const [key, value] of Object.entries(savedEnv)) {
      if (value !== undefined) process.env[key] = value;
    }
  });

  describe("loadConfig", () => {
    it("loads defaults", () => {
      const config = loadConfig();
      expect(config.session.model).toBe("claude-sonnet-4-6");
      expect(config.session.max_tokens).toBe(16384);
      expect(config.persona.theme).toBe("hitchhikers-guide");
      expect(config.persona.role).toBe("dev");
      expect(config.persona.immersion).toBe("high");
      expect(config.statusline.enabled).toBe(true);
      expect(config.telemetry.enabled).toBe(false);
      expect(config.tmux.socket).toBe("ac");
    });

    it("applies CLI overrides", () => {
      const config = loadConfig({
        session: { model: "claude-opus-4-6", max_tokens: 32768 },
      });
      expect(config.session.model).toBe("claude-opus-4-6");
      expect(config.session.max_tokens).toBe(32768);
      // Other defaults preserved
      expect(config.persona.theme).toBe("hitchhikers-guide");
    });

    it("applies env overrides", () => {
      process.env.ACLAUDE_SESSION__MODEL = "claude-opus-4-6";
      process.env.ACLAUDE_PERSONA__THEME = "dune";
      process.env.ACLAUDE_TELEMETRY__ENABLED = "true";

      const config = loadConfig();
      expect(config.session.model).toBe("claude-opus-4-6");
      expect(config.persona.theme).toBe("dune");
      expect(config.telemetry.enabled).toBe(true);
    });

    it("CLI overrides take precedence over env", () => {
      process.env.ACLAUDE_SESSION__MODEL = "claude-haiku-4-5-20251001";
      const config = loadConfig({
        session: { model: "claude-opus-4-6", max_tokens: 16384 },
      });
      expect(config.session.model).toBe("claude-opus-4-6");
    });
  });

  describe("getConfigPaths", () => {
    it("returns expected path structure", () => {
      const paths = getConfigPaths();
      expect(paths.defaults).toContain("config/defaults.toml");
      expect(paths.global).toContain("aclaude/config.toml");
      expect(paths.local).toContain(".aclaude/config.toml");
    });
  });
});
