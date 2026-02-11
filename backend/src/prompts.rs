use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SystemPromptPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub content: &'static str,
}

pub fn builtin_presets() -> Vec<SystemPromptPreset> {
    vec![
        SystemPromptPreset {
            id: "default",
            name: "Default",
            description: "A concise general-purpose assistant prompt.",
            content: include_str!("prompts_content/default.txt"),
        },
        SystemPromptPreset {
            id: "claude-ai",
            name: "Claude AI",
            description: "Comprehensive prompt modeled after Claude.ai behavior guidelines.",
            content: include_str!("prompts_content/claude_ai.txt"),
        },
        SystemPromptPreset {
            id: "claude-code",
            name: "Claude Code",
            description: "Software engineering focused prompt based on Claude Code CLI.",
            content: include_str!("prompts_content/claude_code.txt"),
        },
    ]
}

pub fn get_preset(id: &str) -> Option<SystemPromptPreset> {
    builtin_presets().into_iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_presets_not_empty() {
        assert!(!builtin_presets().is_empty());
    }

    #[test]
    fn test_default_preset_exists() {
        assert!(get_preset("default").is_some());
    }

    #[test]
    fn test_claude_ai_preset_exists() {
        assert!(get_preset("claude-ai").is_some());
    }

    #[test]
    fn test_all_presets_have_unique_ids() {
        let presets = builtin_presets();
        let mut ids: Vec<&str> = presets.iter().map(|p| p.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), presets.len());
    }

    #[test]
    fn test_get_preset_by_id() {
        let preset = get_preset("default").unwrap();
        assert_eq!(preset.id, "default");
        assert!(!preset.content.is_empty());
    }

    #[test]
    fn test_get_nonexistent_preset_returns_none() {
        assert!(get_preset("nonexistent").is_none());
    }

    #[test]
    fn test_claude_ai_contains_behavior_tag() {
        let preset = get_preset("claude-ai").unwrap();
        assert!(preset.content.contains("<claude_behavior>"));
    }

    #[test]
    fn test_claude_code_preset_exists() {
        let preset = get_preset("claude-code").unwrap();
        assert!(preset.content.contains("<claude_code_behavior>"));
    }
}
