use crate::config::UiLanguage;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

struct LocalizerData {
    en: HashMap<String, String>,
    ja: HashMap<String, String>,
}

static LOCALIZER: OnceLock<LocalizerData> = OnceLock::new();
static CURRENT_LANGUAGE: AtomicU8 = AtomicU8::new(0);

fn localizer() -> &'static LocalizerData {
    LOCALIZER.get_or_init(|| LocalizerData {
        en: serde_json::from_str(include_str!("../localize/en.json"))
            .expect("Failed to parse localize/en.json"),
        ja: serde_json::from_str(include_str!("../localize/ja.json"))
            .expect("Failed to parse localize/ja.json"),
    })
}

fn language_code(language: UiLanguage) -> u8 {
    match language {
        UiLanguage::English => 0,
        UiLanguage::Japanese => 1,
    }
}

fn current_language() -> UiLanguage {
    match CURRENT_LANGUAGE.load(Ordering::Relaxed) {
        1 => UiLanguage::Japanese,
        _ => UiLanguage::English,
    }
}

pub fn set_language(language: UiLanguage) {
    CURRENT_LANGUAGE.store(language_code(language), Ordering::Relaxed);
}

pub fn tr(key: &str) -> &str {
    tr_for(current_language(), key)
}

pub fn tr_for<'a>(language: UiLanguage, key: &'a str) -> &'a str {
    let map = match language {
        UiLanguage::English => &localizer().en,
        UiLanguage::Japanese => &localizer().ja,
    };

    map.get(key).map(String::as_str).unwrap_or(key)
}

pub fn tr_args(key: &str, args: &[(&str, &str)]) -> String {
    let mut text = tr(key).to_owned();
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}
