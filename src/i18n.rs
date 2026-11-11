use std::borrow::Cow;
use std::cell::RefCell;

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use unic_langid::LanguageIdentifier;

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

type Bundle = FluentBundle<FluentResource>;

thread_local! {
    static JAPANESE: RefCell<Bundle> = RefCell::new(build_bundle(
        "ja",
        include_str!("../locales/ja.ftl"),
    ));
    static ENGLISH: RefCell<Bundle> = RefCell::new(build_bundle(
        "en",
        include_str!("../locales/en.ftl"),
    ));
}

fn build_bundle(language: &str, source: &str) -> Bundle {
    let language_id = language
        .parse::<LanguageIdentifier>()
        .expect("locale identifier must be valid");
    let resource = FluentResource::try_new(source.to_owned()).unwrap_or_else(|(_, errors)| {
        panic!("{language} Fluent resource must be valid: {errors:?}")
    });
    let mut bundle = FluentBundle::new(vec![language_id]);
    bundle.add_resource(resource).unwrap_or_else(|errors| {
        panic!("{language} Fluent resource has duplicate IDs: {errors:?}")
    });
    // The existing UI formats values directly without bidi isolation marks.
    bundle.set_use_isolating(false);
    bundle
}

fn with_bundle<T>(language: Language, f: impl FnOnce(&Bundle) -> T) -> T {
    match language {
        Language::Japanese => JAPANESE.with(|bundle| f(&bundle.borrow())),
        Language::English => ENGLISH.with(|bundle| f(&bundle.borrow())),
    }
}

fn fluent_id(key: &str) -> Cow<'_, str> {
    if key.contains('.') {
        key.replace('.', "-").into()
    } else {
        key.into()
    }
}

fn format_in(language: Language, key: &str, args: &FluentArgs<'_>) -> Option<String> {
    let id = fluent_id(key);
    with_bundle(language, |bundle| {
        let message = bundle.get_message(&id)?;
        let pattern = message.value()?;
        let mut errors = Vec::new();
        let formatted = bundle.format_pattern(pattern, Some(args), &mut errors);
        if !errors.is_empty() {
            log::warn!(
                "Fluent formatting errors for `{key}` ({}): {errors:?}",
                language.code()
            );
        }
        Some(formatted.into_owned())
    })
}

fn resolve(localized: Option<String>, english: Option<String>, key: &str) -> String {
    localized.or(english).unwrap_or_else(|| missing(key))
}

fn format(language: Language, key: &str, args: &FluentArgs<'_>) -> String {
    let localized = format_in(language, key, args);
    let english = if language == Language::English {
        None
    } else {
        format_in(Language::English, key, args)
    };
    resolve(localized, english, key)
}

/// Looks up a semantic message ID, falling back to English when untranslated.
pub fn t(language: Language, key: &str) -> String {
    format(language, key, &FluentArgs::new())
}

/// Looks up a message and formats named Fluent variables.
pub fn t_args(language: Language, key: &str, args: &[(&str, String)]) -> String {
    let mut fluent_args = FluentArgs::with_capacity(args.len());
    for (name, value) in args {
        fluent_args.set(*name, value.as_str());
    }
    format(language, key, &fluent_args)
}

/// Formats a message with a numeric `count` variable for Fluent's plural selection.
pub fn t_plural(language: Language, key: &str, count: u64, args: &[(&str, String)]) -> String {
    let mut fluent_args = FluentArgs::with_capacity(args.len() + 1);
    for (name, value) in args {
        fluent_args.set(*name, value.as_str());
    }
    fluent_args.set("count", count);
    format(language, key, &fluent_args)
}

fn missing(key: &str) -> String {
    if cfg!(debug_assertions) {
        format!("[MISSING: {key}]")
    } else {
        key.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{build_bundle, missing, resolve, t, t_args, t_plural, Language};

    #[test]
    fn japanese_and_english_fluent_catalogs_have_matching_ids_and_plural_forms() {
        use std::collections::{BTreeMap, BTreeSet};

        fn ids_and_plural_forms(source: &str) -> (Vec<&str>, BTreeMap<String, BTreeSet<String>>) {
            let mut ids = Vec::new();
            let mut plural_forms = BTreeMap::<String, BTreeSet<String>>::new();
            let mut current_plural = None;

            for line in source.lines() {
                if let Some((id, value)) = line.split_once(" = ") {
                    ids.push(id);
                    current_plural = if value == "{ $count ->" {
                        plural_forms.insert(id.to_owned(), BTreeSet::new());
                        Some(id.to_owned())
                    } else {
                        None
                    };
                    continue;
                }

                let Some(id) = current_plural.as_ref() else {
                    continue;
                };
                let variant = line.trim();
                if variant == "}" {
                    current_plural = None;
                    continue;
                }

                let variant = variant.strip_prefix('*').unwrap_or(variant);
                if let Some(category) = variant
                    .strip_prefix('[')
                    .and_then(|variant| variant.split_once(']'))
                    .map(|(category, _)| category)
                {
                    plural_forms
                        .get_mut(id)
                        .expect("plural message was registered")
                        .insert(category.to_owned());
                }
            }

            (ids, plural_forms)
        }

        let japanese = include_str!("../locales/ja.ftl");
        let english = include_str!("../locales/en.ftl");
        let (japanese_ids, japanese_plural_forms) = ids_and_plural_forms(japanese);
        let (english_ids, english_plural_forms) = ids_and_plural_forms(english);

        assert_eq!(japanese_ids, english_ids);
        assert!(japanese_plural_forms
            .values()
            .all(|forms| forms == &["other".to_owned()].into()));
        assert!(english_plural_forms
            .values()
            .all(|forms| forms == &["one".to_owned(), "other".to_owned()].into()));
        build_bundle("ja", japanese);
        build_bundle("en", english);
    }

    #[test]
    fn falls_back_to_english_for_a_missing_translation() {
        assert_eq!(
            resolve(None, Some("English fallback".to_owned()), "missing.ja"),
            "English fallback"
        );
        assert_eq!(resolve(None, None, "missing.ja"), missing("missing.ja"));
    }

    #[test]
    fn formats_named_arguments() {
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
        let unknown = missing("unknown.key");
        if cfg!(debug_assertions) {
            assert_eq!(unknown, "[MISSING: unknown.key]");
        } else {
            assert_eq!(unknown, "unknown.key");
        }
        assert!(t(Language::Japanese, "unknown.key").starts_with("[MISSING:"));
    }
}
