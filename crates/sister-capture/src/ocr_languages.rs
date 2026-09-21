//! Windows OCR language tags that mean Traditional Chinese.
//!
//! `Windows.Media.Ocr` reports whatever BCP-47 tag the installed FOD used.
//! Asking only for `zh-Hant-TW` then falling through to the (often English)
//! user profile is how a machine with `zh-TW` installed still records blank
//! Chinese. Matching lives here so Linux tests can catch a regression without
//! a WinRT OCR engine.

const TRADITIONAL_ALIASES: &[&str] = &["zh-Hant-TW", "zh-Hant", "zh-TW", "zh-HK", "zh-MO"];

/// Tags that are Traditional Chinese OCR, not Simplified and not generic `zh`.
pub fn is_traditional_chinese_ocr(tag: &str) -> bool {
    let tag = tag.trim();
    if tag.is_empty() {
        return false;
    }
    let lower = tag.to_ascii_lowercase();
    lower == "zh-hant"
        || lower.starts_with("zh-hant-")
        || matches!(lower.as_str(), "zh-tw" | "zh-hk" | "zh-mo")
}

/// Preference list plus Traditional Chinese aliases, first-seen order.
///
/// Aliases are inserted immediately after each Traditional preference, before
/// later fallbacks such as `en-US`. An English-only list must not grow a
/// Chinese engine; Simplified tags are never treated as Traditional aliases.
pub fn ocr_language_candidates(preferred: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(preferred.len() + 4);
    for tag in preferred {
        push_unique(&mut out, tag);
        if is_traditional_chinese_ocr(tag) {
            for alias in TRADITIONAL_ALIASES {
                push_unique(&mut out, alias);
            }
        }
    }
    out
}

/// First candidate the `supported` predicate accepts.
///
/// This is the same walk `pick_engine` uses before the user-profile fallback.
/// Tests pass a synthetic available-tag set; production passes WinRT
/// `IsLanguageSupported`.
pub fn first_supported_ocr_language(
    preferred: &[String],
    mut supported: impl FnMut(&str) -> bool,
) -> Option<String> {
    ocr_language_candidates(preferred)
        .into_iter()
        .find(|tag| supported(tag))
}

fn push_unique(out: &mut Vec<String>, tag: &str) {
    if !out.iter().any(|existing| existing == tag) {
        out.push(tag.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn installed<'a>(list: &'a [&'a str]) -> impl FnMut(&str) -> bool + 'a {
        move |tag| list.iter().any(|have| have.eq_ignore_ascii_case(tag))
    }

    #[test]
    fn traditional_tags_are_not_simplified_or_generic() {
        for tag in [
            "zh-Hant-TW",
            "zh-Hant",
            "zh-TW",
            "zh-HK",
            "zh-MO",
            "zh-Hant-HK",
        ] {
            assert!(is_traditional_chinese_ocr(tag), "{tag}");
        }
        for tag in ["zh", "zh-CN", "zh-Hans", "zh-Hans-CN", "en-US", ""] {
            assert!(!is_traditional_chinese_ocr(tag), "{tag}");
        }
    }

    #[test]
    fn aliases_are_inserted_before_later_english_fallback() {
        let got = ocr_language_candidates(&tags(&["zh-Hant-TW", "zh-Hant", "en-US"]));
        assert_eq!(
            got,
            tags(&["zh-Hant-TW", "zh-Hant", "zh-TW", "zh-HK", "zh-MO", "en-US"])
        );
        let zh_tw = got.iter().position(|t| t == "zh-TW").unwrap();
        let en = got.iter().position(|t| t == "en-US").unwrap();
        assert!(zh_tw < en, "zh-TW must be tried before en-US: {got:?}");
    }

    #[test]
    fn english_only_preference_does_not_invent_chinese() {
        assert_eq!(ocr_language_candidates(&tags(&["en-US"])), tags(&["en-US"]));
    }

    #[test]
    fn simplified_preference_is_not_rewritten_as_traditional() {
        assert_eq!(
            ocr_language_candidates(&tags(&["zh-Hans-CN", "en-US"])),
            tags(&["zh-Hans-CN", "en-US"])
        );
    }

    #[test]
    fn traditional_preference_selects_zh_tw_ahead_of_installed_english() {
        let preferred = tags(&["zh-Hant-TW", "zh-Hant", "en-US"]);
        let chosen = first_supported_ocr_language(&preferred, installed(&["en-US", "zh-TW"]));
        assert_eq!(
            chosen.as_deref(),
            Some("zh-TW"),
            "an English profile with Language.OCR~~~zh-TW must not keep en-US"
        );
    }

    #[test]
    fn english_first_preference_keeps_english_when_both_are_installed() {
        let preferred = tags(&["en-US", "zh-Hant-TW"]);
        let chosen = first_supported_ocr_language(&preferred, installed(&["zh-TW", "en-US"]));
        assert_eq!(chosen.as_deref(), Some("en-US"));
    }

    #[test]
    fn traditional_preference_falls_through_when_only_english_is_installed() {
        let preferred = tags(&["zh-Hant-TW", "en-US"]);
        let chosen = first_supported_ocr_language(&preferred, installed(&["en-US"]));
        assert_eq!(chosen.as_deref(), Some("en-US"));
    }

    #[test]
    fn english_only_preference_does_not_select_installed_zh_tw() {
        let preferred = tags(&["en-US"]);
        let chosen = first_supported_ocr_language(&preferred, installed(&["en-US", "zh-TW"]));
        assert_eq!(chosen.as_deref(), Some("en-US"));
    }
}
