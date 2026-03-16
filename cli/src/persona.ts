import { readFileSync, readdirSync, existsSync } from "node:fs";
import { join, basename } from "node:path";
import { parse as parseYaml } from "yaml";

export interface PersonaAgent {
  character: string;
  shortName?: string;
  style: string;
  expertise: string;
  role: string;
  trait: string;
  quirks: string[];
  catchphrases: string[];
  emoji?: string;
  ocean: { O: number; C: number; E: number; A: number; N: number };
}

export interface PersonaTheme {
  name: string;
  slug: string;
  description: string;
  source: string;
  user_title?: string;
  character_immersion?: string;
  dimensions?: Record<string, string>;
  spinner_verbs?: string[];
  agents: Record<string, PersonaAgent>;
}

function getThemesDir(): string {
  let dir = new URL(".", import.meta.url).pathname;
  for (let i = 0; i < 10; i++) {
    const candidate = join(dir, "personas", "themes");
    if (existsSync(candidate)) return candidate;
    const parent = join(dir, "..");
    if (parent === dir) break;
    dir = parent;
  }
  return join(process.cwd(), "personas", "themes");
}

export function listThemes(): string[] {
  const dir = getThemesDir();
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .filter((f) => f.endsWith(".yaml"))
    .map((f) => f.replace(/\.yaml$/, ""))
    .sort();
}

export function loadTheme(slug: string): PersonaTheme | null {
  const dir = getThemesDir();
  const filePath = join(dir, `${slug}.yaml`);
  if (!existsSync(filePath)) return null;

  try {
    const raw = parseYaml(readFileSync(filePath, "utf-8"));
    if (!raw || typeof raw !== "object") return null;

    return {
      name: raw.theme?.name || slug,
      slug,
      description: raw.theme?.description || "",
      source: raw.theme?.source || "",
      user_title: raw.theme?.user_title,
      character_immersion: raw.theme?.character_immersion,
      dimensions: raw.theme?.dimensions,
      spinner_verbs: raw.theme?.spinner_verbs,
      agents: raw.agents || {},
    };
  } catch {
    return null;
  }
}

export function getAgent(theme: PersonaTheme, role: string): PersonaAgent | null {
  return theme.agents[role] || null;
}

export function buildSystemPrompt(
  theme: PersonaTheme,
  agent: PersonaAgent,
  immersion: "high" | "medium" | "low" | "none",
): string {
  if (immersion === "none") {
    return "You are a helpful software engineering assistant.";
  }

  const parts: string[] = [];

  if (immersion === "high") {
    parts.push(`You are ${agent.character} from ${theme.name}.`);
    parts.push(`Style: ${agent.style}`);
    parts.push(`Expertise: ${agent.expertise}`);
    if (agent.trait) parts.push(`Key trait: ${agent.trait}`);
    if (agent.quirks?.length) parts.push(`Quirks: ${agent.quirks.join("; ")}`);
    if (agent.catchphrases?.length) {
      parts.push(`Catchphrases you may use: ${agent.catchphrases.map((c) => `"${c}"`).join(", ")}`);
    }
    if (theme.user_title) parts.push(`Address the user as "${theme.user_title}".`);
  } else if (immersion === "medium") {
    parts.push(`You are ${agent.character}, a ${agent.expertise} assistant.`);
    parts.push(`Style: ${agent.style}`);
    if (agent.catchphrases?.length) {
      parts.push(`You occasionally say: "${agent.catchphrases[0]}"`);
    }
  } else {
    // low
    parts.push(`You are a helpful software engineering assistant with the personality of ${agent.character}.`);
    parts.push(`Expertise: ${agent.expertise}`);
  }

  parts.push("");
  parts.push("You are a software engineering assistant. Help the user with their coding tasks.");

  return parts.join("\n");
}
