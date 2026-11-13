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

/// Detects the user's system language using GLib/POSIX locale priority (`LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, `LANG`).
pub fn detect_system_language() -> Language {
    for name in glib::language_names() {
        let lower = name.to_lowercase();
        if lower.starts_with("es") {
            return Language::Spanish;
        }
        if lower.starts_with("en") {
            return Language::English;
        }
    }
    Language::English
}

pub fn init() {
    if let Some(lang) = load_saved_language() {
        set_language_internal(lang);
    } else {
        set_language_internal(detect_system_language());
    }
}

/// Returns `0` for Auto (System), `1` for Spanish, `2` for English.
pub fn saved_preference_index() -> u32 {
    match load_saved_language() {
        None => 0,
        Some(Language::Spanish) => 1,
        Some(Language::English) => 2,
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
    save_language_str(lang.code());
}

pub fn set_language_auto() {
    set_language_internal(detect_system_language());
    save_language_str("auto");
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

fn save_language_str(val: &str) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, val);
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
        let prev = current_language();
        set_language_internal(Language::Spanish);
        assert_eq!(current_language(), Language::Spanish);
        assert_eq!(tr!("Hola", "Hello"), "Hola");

        set_language_internal(Language::English);
        assert_eq!(current_language(), Language::English);
        assert_eq!(tr!("Hola", "Hello"), "Hello");

        set_language_internal(prev);
    }
}
