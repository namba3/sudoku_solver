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
    resolve_message(
        language,
        key,
        plural_count,
        catalog(language).get(key),
        catalog(Language::English).get(key),
    )
}

fn resolve_message(
    language: Language,
    key: &str,
    plural_count: Option<u64>,
    localized: Option<&Value>,
    english: Option<&Value>,
) -> String {
    let (value, message_language) = localized
        .map(|value| (Some(value), language))
        .unwrap_or((english, Language::English));

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

#[cfg(test)]
mod tests {
    use super::{catalog, resolve_message, t_args, t_plural, Language};
    use serde_json::json;

    #[test]
    fn japanese_and_english_catalogs_have_matching_keys_and_message_shapes() {
        let japanese = catalog(Language::Japanese);
        let english = catalog(Language::English);

        assert_eq!(japanese.len(), english.len());
        for (key, japanese_value) in japanese {
            let english_value = english
                .get(key)
                .unwrap_or_else(|| panic!("missing English key: {key}"));
            assert_eq!(
                japanese_value.is_object(),
                english_value.is_object(),
                "message shape differs for {key}"
            );
            if japanese_value.is_object() {
                assert!(
                    japanese_value.get("other").is_some(),
                    "Japanese plural message lacks `other`: {key}"
                );
                assert!(
                    english_value.get("other").is_some(),
                    "English plural message lacks `other`: {key}"
                );
            }
        }
    }

    #[test]
    fn falls_back_to_english_for_a_missing_translation() {
        let english = json!("English fallback");

        assert_eq!(
            resolve_message(Language::Japanese, "missing.ja", None, None, Some(&english)),
            "English fallback"
        );
    }

    #[test]
    fn english_fallback_uses_english_plural_rules() {
        let english = json!({"one": "{count} item", "other": "{count} items"});

        assert_eq!(
            resolve_message(
                Language::Japanese,
                "missing.ja",
                Some(1),
                None,
                Some(&english)
            ),
            "{count} item"
        );
        assert_eq!(
            resolve_message(
                Language::Japanese,
                "missing.ja",
                Some(2),
                None,
                Some(&english)
            ),
            "{count} items"
        );
    }

    #[test]
    fn interpolates_named_arguments_in_a_complete_message() {
        assert_eq!(
            t_args(
                Language::Japanese,
                "parse.line_count",
                &[("actual", "3".to_owned())]
            ),
            "9行必要ですが、3行あります。"
        );
        assert_eq!(
            t_args(
                Language::English,
                "parse.line_count",
                &[("actual", "3".to_owned())]
            ),
            "Expected 9 lines, found 3."
        );
    }

    #[test]
    fn chooses_english_and_japanese_plural_forms() {
        assert_eq!(
            t_plural(
                Language::English,
                "search.cancelled",
                1,
                &[("seconds", "1.00".to_owned())]
            ),
            "Search cancelled after finding 1 solution in 1.00 s."
        );
        assert_eq!(
            t_plural(
                Language::English,
                "search.cancelled",
                2,
                &[("seconds", "1.00".to_owned())]
            ),
            "Search cancelled after finding 2 solutions in 1.00 s."
        );
        assert_eq!(
            t_plural(
                Language::Japanese,
                "search.cancelled",
                2,
                &[("seconds", "1.00".to_owned())]
            ),
            "解を 2 件見つけた時点で探索を中断しました（1.00 秒）。"
        );
    }

    #[test]
    fn marks_unknown_keys_in_debug_builds() {
        let unknown = resolve_message(Language::Japanese, "unknown.key", None, None, None);

        if cfg!(debug_assertions) {
            assert_eq!(unknown, "[MISSING: unknown.key]");
        } else {
            assert_eq!(unknown, "unknown.key");
        }
    }
}
