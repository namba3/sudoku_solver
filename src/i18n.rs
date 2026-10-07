use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;

#[derive(Clone, Copy, PartialEq)]
pub enum Language {
    Japanese,
    English,
}

impl Language {
    pub fn toggle(self) -> Self {
        match self {
            Self::Japanese => Self::English,
            Self::English => Self::Japanese,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Japanese => "ja",
            Self::English => "en",
        }
    }
}

fn catalog(language: Language) -> &'static HashMap<String, Value> {
    static JAPANESE: OnceLock<HashMap<String, Value>> = OnceLock::new();
    static ENGLISH: OnceLock<HashMap<String, Value>> = OnceLock::new();

    let (cache, source) = match language {
        Language::Japanese => (&JAPANESE, include_str!("../locales/ja.json")),
        Language::English => (&ENGLISH, include_str!("../locales/en.json")),
    };

    cache.get_or_init(|| serde_json::from_str(source).expect("locale catalog must be valid JSON"))
}

/// Looks up a semantic message key, falling back to English for untranslated entries.
pub fn t(language: Language, key: &str) -> String {
    message(language, key, None)
}

/// Looks up a message and substitutes named values such as `{count}`.
pub fn t_args(language: Language, key: &str, args: &[(&str, String)]) -> String {
    interpolate(message(language, key, None), args)
}

/// Selects the locale's plural form before substituting `{count}`.
pub fn t_plural(language: Language, key: &str, count: u64, args: &[(&str, String)]) -> String {
    let mut values = args.to_vec();
    values.push(("count", count.to_string()));
    let selected = message(language, key, Some(count));
    interpolate(selected, &values)
}

fn message(language: Language, key: &str, plural_count: Option<u64>) -> String {
    let (value, message_language) = match catalog(language).get(key) {
        Some(value) => (Some(value), language),
        None => (catalog(Language::English).get(key), Language::English),
    };

    match value {
        Some(Value::String(message)) => message.clone(),
        Some(Value::Object(forms)) => plural_count
            .map(|count| match message_language {
                Language::Japanese => "other",
                Language::English if count == 1 => "one",
                Language::English => "other",
            })
            .and_then(|category| forms.get(category))
            .or_else(|| forms.get("other"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| missing(key)),
        _ => missing(key),
    }
}

fn missing(key: &str) -> String {
    if cfg!(debug_assertions) {
        format!("[MISSING: {key}]")
    } else {
        key.to_owned()
    }
}

fn interpolate(mut message: String, args: &[(&str, String)]) -> String {
    for (name, value) in args {
        message = message.replace(&format!("{{{name}}}"), value);
    }
    message
}
