use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{ForestageError, Result};

include!(concat!(env!("OUT_DIR"), "/themes_embedded.rs"));

/// OCEAN personality model scores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ocean {
    #[serde(alias = "O")]
    pub openness: f64,
    #[serde(alias = "C")]
    pub conscientiousness: f64,
    #[serde(alias = "E")]
    pub extraversion: f64,
    #[serde(alias = "A")]
    pub agreeableness: f64,
    #[serde(alias = "N")]
    pub neuroticism: f64,
}

/// A single agent/character within a theme.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaAgent {
    pub character: String,
    #[serde(rename = "shortName")]
    pub short_name: Option<String>,
    #[serde(default)]
    pub style: String,
    #[serde(default)]
    pub expertise: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub r#trait: String,
    #[serde(default)]
    pub quirks: Vec<String>,
    #[serde(default)]
    pub catchphrases: Vec<String>,
    pub emoji: Option<String>,
    pub ocean: Option<Ocean>,
}

/// Theme metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeInfo {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub source: String,
    pub user_title: Option<String>,
    pub character_immersion: Option<String>,
    pub spinner_verbs: Option<Vec<String>>,
    pub dimensions: Option<HashMap<String, String>>,
}

/// Top-level theme file structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeFile {
    #[serde(default)]
    pub category: String,
    pub theme: ThemeInfo,
    #[serde(default)]
    pub agents: HashMap<String, PersonaAgent>,
}

/// List all available theme slugs (from embedded data).
pub fn list_themes() -> Vec<String> {
    let themes = embedded_themes();
    let mut slugs: Vec<String> = themes.keys().map(ToString::to_string).collect();
    slugs.sort();
    slugs
}

/// Load a theme by slug.
pub fn load_theme(slug: &str) -> Result<ThemeFile> {
    let themes = embedded_themes();
    let yaml = themes
        .get(slug)
        .ok_or_else(|| ForestageError::ThemeNotFound {
            slug: slug.to_string(),
        })?;

    let theme: ThemeFile = serde_yaml::from_str(yaml).map_err(|e| ForestageError::Yaml {
        path: format!("embedded:{slug}"),
        source: e,
    })?;

    Ok(theme)
}

/// Get a specific agent by role within a theme.
pub fn get_agent<'a>(theme: &'a ThemeFile, role: &str) -> Result<&'a PersonaAgent> {
    theme
        .agents
        .get(role)
        .ok_or_else(|| ForestageError::RoleNotFound {
            role: role.to_string(),
            theme: theme.theme.name.clone(),
        })
}

/// Build system prompt text based on immersion level.
pub fn build_system_prompt(theme: &ThemeFile, agent: &PersonaAgent, immersion: &str) -> String {
    match immersion {
        "high" => {
            let mut parts = vec![format!("You are {}, {}.", agent.character, agent.style)];
            if !agent.expertise.is_empty() {
                parts.push(format!("Expertise: {}", agent.expertise));
            }
            if !agent.r#trait.is_empty() {
                parts.push(format!("Key trait: {}", agent.r#trait));
            }
            if !agent.quirks.is_empty() {
                parts.push(format!("Quirks: {}", agent.quirks.join("; ")));
            }
            if !agent.catchphrases.is_empty() {
                parts.push(format!("Catchphrases: {}", agent.catchphrases.join("; ")));
            }
            if let Some(title) = &theme.theme.user_title {
                parts.push(format!("Address the user as: {title}"));
            }
            parts.join("\n")
        }
        "medium" => {
            let mut prompt = format!("You are {}, {}.", agent.character, agent.style);
            if let Some(phrase) = agent.catchphrases.first() {
                prompt.push_str(&format!(" Signature phrase: \"{phrase}\""));
            }
            prompt
        }
        "low" => {
            let mut prompt = format!(
                "Bring a touch of {}'s personality to your responses.",
                agent.character
            );
            if !agent.expertise.is_empty() {
                prompt.push_str(&format!(" Focus area: {}", agent.expertise));
            }
            prompt
        }
        _ => String::new(), // "none" or unknown
    }
}
