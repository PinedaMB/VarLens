use gtk4::glib;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Spanish = 0,
    English = 1,
}

impl Language {
    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => Language::English,
            _ => Language::Spanish,
        }
    }

    pub fn as_u8(&self) -> u8 {
        match self {
            Language::Spanish => 0,
            Language::English => 1,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Language::Spanish => "es",
            Language::English => "en",
        }
    }

    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            Language::Spanish => "Español",
            Language::English => "English",
        }
    }
}

static CURRENT_LANG: AtomicU8 = AtomicU8::new(0);

pub fn init() {
    if let Some(lang) = load_saved_language() {
        set_language_internal(lang);
    } else if let Ok(lang_env) = std::env::var("LANG") {
        if lang_env.starts_with("es") {
            set_language_internal(Language::Spanish);
        } else {
            set_language_internal(Language::English);
        }
    } else {
        set_language_internal(Language::Spanish);
    }
}

pub fn current_language() -> Language {
    Language::from_u8(CURRENT_LANG.load(Ordering::Relaxed))
}

fn set_language_internal(lang: Language) {
    CURRENT_LANG.store(lang.as_u8(), Ordering::Relaxed);
}

pub fn set_language(lang: Language) {
    set_language_internal(lang);
    save_language(lang);
}

fn config_path() -> std::path::PathBuf {
    glib::user_config_dir().join("varlens").join("language.txt")
}

fn load_saved_language() -> Option<Language> {
    let path = config_path();
    if let Ok(content) = std::fs::read_to_string(path) {
        let trimmed = content.trim();
        if trimmed == "en" {
            return Some(Language::English);
        } else if trimmed == "es" {
            return Some(Language::Spanish);
        }
    }
    None
}

fn save_language(lang: Language) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, lang.code());
}

/// Returns either the Spanish or English text according to the active language.
#[inline]
pub fn tr(es: &'static str, en: &'static str) -> &'static str {
    match current_language() {
        Language::Spanish => es,
        Language::English => en,
    }
}

#[macro_export]
macro_rules! tr {
    ($es:expr, $en:expr) => {
        $crate::i18n::tr($es, $en)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_switch_and_tr() {
        set_language(Language::Spanish);
        assert_eq!(current_language(), Language::Spanish);
        assert_eq!(tr!("Hola", "Hello"), "Hola");

        set_language(Language::English);
        assert_eq!(current_language(), Language::English);
        assert_eq!(tr!("Hola", "Hello"), "Hello");

        // Restore to Spanish
        set_language(Language::Spanish);
    }
}
