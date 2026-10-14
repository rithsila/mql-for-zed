use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimaryLanguage {
    #[serde(rename = "en")]
    English,
    #[serde(rename = "km")]
    Khmer,
}

impl Default for PrimaryLanguage {
    fn default() -> Self {
        PrimaryLanguage::English
    }
}

impl PrimaryLanguage {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            PrimaryLanguage::English => "en",
            PrimaryLanguage::Khmer => "km",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageConfig {
    #[serde(default = "default_true")]
    pub english: bool,
    #[serde(default = "default_false")]
    pub khmer: bool,
}

impl Default for LanguageConfig {
    fn default() -> Self {
        Self {
            english: true,
            khmer: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoverConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub languages: LanguageConfig,
    #[serde(default)]
    pub primary_language: PrimaryLanguage,
    #[serde(default = "default_true")]
    pub show_source: bool,
    #[serde(default = "default_true")]
    pub show_documentation: bool,
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

impl Default for HoverConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            languages: LanguageConfig::default(),
            primary_language: PrimaryLanguage::default(),
            show_source: true,
            show_documentation: true,
        }
    }
}

impl HoverConfig {
    /// Determines the effective language to use for primary ordering and labels:
    /// - If both languages are enabled, respect `primary_language`.
    /// - If only Khmer is enabled, use Khmer.
    /// - If only English is enabled, use English.
    /// - If both are disabled (facts-only mode), fall back to English for structural labels.
    pub fn effective_primary(&self) -> PrimaryLanguage {
        if self.languages.english && self.languages.khmer {
            self.primary_language
        } else if self.languages.khmer && !self.languages.english {
            PrimaryLanguage::Khmer
        } else {
            PrimaryLanguage::English
        }
    }

    /// Are both languages disabled? (Facts-only mode: signatures and metadata, no prose)
    pub fn is_facts_only(&self) -> bool {
        !self.languages.english && !self.languages.khmer
    }

    /// Are both languages enabled? (Bilingual mode)
    pub fn is_bilingual(&self) -> bool {
        self.languages.english && self.languages.khmer
    }
}

/// Parse and validate `HoverConfig` from LSP `initialization_options`.
/// Missing fields receive defaults.
/// Unknown fields are ignored for forward compatibility.
/// Present fields with invalid types trigger warnings on stderr without discarding valid sibling fields.
pub fn from_initialization_options(options: Option<&Value>) -> HoverConfig {
    let mut config = HoverConfig::default();

    let hover_val = match options.and_then(|o| o.get("hover")) {
        Some(v) => v,
        None => return config,
    };

    let obj = match hover_val.as_object() {
        Some(o) => o,
        None => {
            eprintln!(
                "[mql-lsp] Warning: `hover` configuration must be a JSON object; using defaults"
            );
            return config;
        }
    };

    if let Some(v) = obj.get("enabled") {
        if let Some(b) = v.as_bool() {
            config.enabled = b;
        } else {
            eprintln!(
                "[mql-lsp] Warning: `hover.enabled` must be a boolean; using default ({})",
                config.enabled
            );
        }
    }

    if let Some(v) = obj.get("languages") {
        if let Some(langs_obj) = v.as_object() {
            if let Some(en_val) = langs_obj.get("english") {
                if let Some(b) = en_val.as_bool() {
                    config.languages.english = b;
                } else {
                    eprintln!(
                        "[mql-lsp] Warning: `hover.languages.english` must be a boolean; using default ({})",
                        config.languages.english
                    );
                }
            }
            if let Some(km_val) = langs_obj.get("khmer") {
                if let Some(b) = km_val.as_bool() {
                    config.languages.khmer = b;
                } else {
                    eprintln!(
                        "[mql-lsp] Warning: `hover.languages.khmer` must be a boolean; using default ({})",
                        config.languages.khmer
                    );
                }
            }
        } else {
            eprintln!("[mql-lsp] Warning: `hover.languages` must be a JSON object; using default languages");
        }
    }

    if let Some(v) = obj.get("primaryLanguage") {
        if let Some(s) = v.as_str() {
            match s.to_ascii_lowercase().as_str() {
                "en" | "english" => config.primary_language = PrimaryLanguage::English,
                "km" | "khmer" => config.primary_language = PrimaryLanguage::Khmer,
                other => {
                    eprintln!(
                        "[mql-lsp] Warning: invalid `hover.primaryLanguage` '{}' (expected 'en' or 'km'); using 'en'",
                        other
                    );
                }
            }
        } else {
            eprintln!(
                "[mql-lsp] Warning: `hover.primaryLanguage` must be a string; using default 'en'"
            );
        }
    }

    if let Some(v) = obj.get("showSource") {
        if let Some(b) = v.as_bool() {
            config.show_source = b;
        } else {
            eprintln!(
                "[mql-lsp] Warning: `hover.showSource` must be a boolean; using default ({})",
                config.show_source
            );
        }
    }

    if let Some(v) = obj.get("showDocumentation") {
        if let Some(b) = v.as_bool() {
            config.show_documentation = b;
        } else {
            eprintln!(
                "[mql-lsp] Warning: `hover.showDocumentation` must be a boolean; using default ({})",
                config.show_documentation
            );
        }
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_default_config() {
        let config = from_initialization_options(None);
        assert!(config.enabled);
        assert!(config.languages.english);
        assert!(!config.languages.khmer);
        assert_eq!(config.primary_language, PrimaryLanguage::English);
        assert_eq!(config.effective_primary(), PrimaryLanguage::English);
        assert!(config.show_source);
        assert!(config.show_documentation);
        assert!(!config.is_facts_only());
        assert!(!config.is_bilingual());
    }

    #[test]
    fn test_six_configuration_modes() {
        // 1. English only
        let opts = json!({
            "hover": {
                "enabled": true,
                "languages": { "english": true, "khmer": false },
                "primaryLanguage": "en"
            }
        });
        let c = from_initialization_options(Some(&opts));
        assert!(c.enabled && c.languages.english && !c.languages.khmer);
        assert_eq!(c.effective_primary(), PrimaryLanguage::English);

        // 2. Khmer only
        let opts = json!({
            "hover": {
                "enabled": true,
                "languages": { "english": false, "khmer": true },
                "primaryLanguage": "km"
            }
        });
        let c = from_initialization_options(Some(&opts));
        assert!(c.enabled && !c.languages.english && c.languages.khmer);
        assert_eq!(c.effective_primary(), PrimaryLanguage::Khmer);

        // 2b. Khmer only with primaryLanguage "en" -> should resolve effective to Khmer
        let opts = json!({
            "hover": {
                "enabled": true,
                "languages": { "english": false, "khmer": true },
                "primaryLanguage": "en"
            }
        });
        let c = from_initialization_options(Some(&opts));
        assert_eq!(c.effective_primary(), PrimaryLanguage::Khmer);

        // 3. Both, Khmer first
        let opts = json!({
            "hover": {
                "enabled": true,
                "languages": { "english": true, "khmer": true },
                "primaryLanguage": "km"
            }
        });
        let c = from_initialization_options(Some(&opts));
        assert!(c.enabled && c.languages.english && c.languages.khmer);
        assert_eq!(c.effective_primary(), PrimaryLanguage::Khmer);
        assert!(c.is_bilingual());

        // 4. Both, English first
        let opts = json!({
            "hover": {
                "enabled": true,
                "languages": { "english": true, "khmer": true },
                "primaryLanguage": "en"
            }
        });
        let c = from_initialization_options(Some(&opts));
        assert!(c.enabled && c.languages.english && c.languages.khmer);
        assert_eq!(c.effective_primary(), PrimaryLanguage::English);
        assert!(c.is_bilingual());

        // 5. Signature/facts only
        let opts = json!({
            "hover": {
                "enabled": true,
                "languages": { "english": false, "khmer": false },
                "primaryLanguage": "en"
            }
        });
        let c = from_initialization_options(Some(&opts));
        assert!(c.enabled && !c.languages.english && !c.languages.khmer);
        assert!(c.is_facts_only());
        assert_eq!(c.effective_primary(), PrimaryLanguage::English);

        // 6. Hover off
        let opts = json!({
            "hover": {
                "enabled": false,
                "languages": { "english": true, "khmer": true }
            }
        });
        let c = from_initialization_options(Some(&opts));
        assert!(!c.enabled);
    }

    #[test]
    fn test_partial_and_invalid_fields() {
        let opts = json!({
            "hover": {
                "languages": { "khmer": true }, // english omitted -> defaults to true
                "primaryLanguage": 12345,        // invalid type -> fallback to en
                "unknownField": "ignored"        // forward compatibility
            }
        });
        let c = from_initialization_options(Some(&opts));
        assert!(c.enabled);
        assert!(c.languages.english);
        assert!(c.languages.khmer);
        assert_eq!(c.primary_language, PrimaryLanguage::English);
    }
}
