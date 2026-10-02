use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) enum Language {
    #[default]
    English,
    Spanish,
    Portuguese,
    Russian,
    Japanese,
}

impl Language {
    pub(crate) const ALL: &'static [Self] = &[
        Self::English,
        Self::Spanish,
        Self::Portuguese,
        Self::Russian,
        Self::Japanese,
    ];

    fn catalog(self) -> &'static str {
        match self {
            Self::English => include_str!("../locales/en.json"),
            Self::Spanish => include_str!("../locales/es.json"),
            Self::Portuguese => include_str!("../locales/pt.json"),
            Self::Russian => include_str!("../locales/ru.json"),
            Self::Japanese => include_str!("../locales/ja.json"),
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::English => "English",
            Self::Spanish => "Español",
            Self::Portuguese => "Português",
            Self::Russian => "Русский",
            Self::Japanese => "日本語",
        })
    }
}

type Catalog = HashMap<String, String>;

static IS_ENGLISH: AtomicBool = AtomicBool::new(true);
static ACTIVE: LazyLock<RwLock<Catalog>> = LazyLock::new(|| RwLock::new(Catalog::new()));

fn catalog_for(language: Language) -> Catalog {
    serde_json::from_str::<Catalog>(language.catalog())
        .unwrap_or_default()
        .into_iter()
        .filter(|(key, value)| key != value && !value.trim().is_empty())
        .collect()
}

pub(crate) fn set_language(language: Language) {
    let catalog = if language == Language::English {
        Catalog::new()
    } else {
        catalog_for(language)
    };
    if let Ok(mut active) = ACTIVE.write() {
        *active = catalog;
    }
    IS_ENGLISH.store(language == Language::English, Ordering::Relaxed);
}

pub(crate) fn tr(text: &str) -> String {
    if IS_ENGLISH.load(Ordering::Relaxed) {
        return text.to_string();
    }
    ACTIVE
        .read()
        .ok()
        .and_then(|active| active.get(text).cloned())
        .unwrap_or_else(|| text.to_string())
}

pub(crate) fn tr_with(template: &str, args: &[(&str, &str)]) -> String {
    let mut text = tr(template);
    for (key, value) in args {
        text = text.replace(key, value);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(language: Language) -> Catalog {
        serde_json::from_str(language.catalog()).expect("valid catalog")
    }

    #[test]
    fn every_catalog_parses() {
        for language in Language::ALL {
            let _ = raw(*language);
        }
    }

    #[test]
    fn english_catalog_is_the_canonical_string_list() {
        for (key, value) in raw(Language::English) {
            assert_eq!(key, value, "en.json must map each string to itself");
        }
    }

    #[test]
    fn spanish_catalog_translates_common_strings() {
        let spanish = catalog_for(Language::Spanish);
        for (english, expected) in [
            ("Settings", "Ajustes"),
            ("Connect", "Conectar"),
            ("Run query", "Ejecutar consulta"),
            ("No results.", "Sin resultados."),
        ] {
            assert_eq!(spanish.get(english).map(String::as_str), Some(expected));
        }
    }

    #[test]
    fn translations_only_use_known_strings() {
        let english = raw(Language::English);
        for language in Language::ALL {
            if *language == Language::English {
                continue;
            }
            for key in raw(*language).keys() {
                assert!(
                    english.contains_key(key),
                    "{language}: `{key}` is not in en.json"
                );
            }
        }
    }
}
