use super::{LOCALES, match_locale};

#[test]
fn system_locales_map_to_supported_languages() {
    for (tag, expected) in [
        ("en-US", "en"),
        ("fr-FR", "en"),
        ("", "en"),
        ("ja-JP", "ja"),
        ("ko_KR", "ko"),
        ("zh-CN", "zh-CN"),
        ("zh-Hans-CN", "zh-CN"),
        ("zh-SG", "zh-CN"),
        ("zh-TW", "zh-TW"),
        ("zh-Hant-TW", "zh-TW"),
        ("zh-HK", "zh-TW"),
    ] {
        assert_eq!(match_locale(tag), expected, "{tag}");
    }
}

#[test]
fn every_key_is_translated_into_every_language() {
    let backend = crate::_rust_i18n_backend();
    let keys = |locale| {
        let mut keys: Vec<String> = backend
            .messages_for_locale(locale)
            .unwrap_or_default()
            .into_iter()
            .map(|(key, _)| key.into_owned())
            .collect();
        keys.sort();
        keys
    };
    let english = keys("en");
    assert!(!english.is_empty());
    for (locale, _) in LOCALES {
        assert_eq!(keys(locale), english, "{locale}");
    }
}
