//! Site-wide CMS interface language catalog, independent of content locales.

pub const CMS_CATALOG_VERSION: &str = "v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct CmsLocaleOption {
    pub code: &'static str,
    pub bcp47: &'static str,
    pub native_name: &'static str,
    pub english_name: &'static str,
    pub direction: &'static str,
    pub installed: bool,
    pub translation_coverage: u8,
}

pub const CMS_LANGUAGE_CATALOG: &[CmsLocaleOption] = &[
    CmsLocaleOption {
        code: "en",
        bcp47: "en-US",
        native_name: "`English`",
        english_name: "`English`",
        direction: "ltr",
        installed: true,
        translation_coverage: 100,
    },
    CmsLocaleOption {
        code: "vi",
        bcp47: "vi-VN",
        native_name: "`Tiếng Việt`",
        english_name: "`Vietnamese`",
        direction: "ltr",
        installed: true,
        translation_coverage: 100,
    },
    CmsLocaleOption {
        code: "ar",
        bcp47: "ar",
        native_name: "`العربية`",
        english_name: "`Arabic`",
        direction: "rtl",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "bg",
        bcp47: "bg-BG",
        native_name: "`Български`",
        english_name: "`Bulgarian`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "cs",
        bcp47: "cs-CZ",
        native_name: "`Čeština`",
        english_name: "`Czech`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "da",
        bcp47: "da-DK",
        native_name: "`Dansk`",
        english_name: "`Danish`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "de",
        bcp47: "de-DE",
        native_name: "`Deutsch`",
        english_name: "`German`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "el",
        bcp47: "el",
        native_name: "`Ελληνικά`",
        english_name: "`Greek`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "es",
        bcp47: "es-ES",
        native_name: "`Español`",
        english_name: "`Spanish`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "fi",
        bcp47: "fi",
        native_name: "`Suomi`",
        english_name: "`Finnish`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "fr",
        bcp47: "fr-FR",
        native_name: "`Français`",
        english_name: "`French`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "he",
        bcp47: "he-IL",
        native_name: "`עברית`",
        english_name: "`Hebrew`",
        direction: "rtl",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "hi",
        bcp47: "hi-IN",
        native_name: "`हिन्दी`",
        english_name: "`Hindi`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "hu",
        bcp47: "hu-HU",
        native_name: "`Magyar`",
        english_name: "`Hungarian`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "id",
        bcp47: "id-ID",
        native_name: "`Bahasa Indonesia`",
        english_name: "`Indonesian`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "it",
        bcp47: "it-IT",
        native_name: "`Italiano`",
        english_name: "`Italian`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "ja",
        bcp47: "ja",
        native_name: "`日本語`",
        english_name: "`Japanese`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "ko",
        bcp47: "ko-KR",
        native_name: "`한국어`",
        english_name: "`Korean`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "ms",
        bcp47: "ms-MY",
        native_name: "`Bahasa Melayu`",
        english_name: "`Malay`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "nb",
        bcp47: "nb-NO",
        native_name: "`Norsk bokmål`",
        english_name: "`Norwegian Bokmål`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "nl",
        bcp47: "nl-NL",
        native_name: "`Nederlands`",
        english_name: "`Dutch`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "pl",
        bcp47: "pl-PL",
        native_name: "`Polski`",
        english_name: "`Polish`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "pt-br",
        bcp47: "pt-BR",
        native_name: "`Português do Brasil`",
        english_name: "`Portuguese (Brazil)`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "pt-pt",
        bcp47: "pt-PT",
        native_name: "`Português`",
        english_name: "`Portuguese (Portugal)`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "ro",
        bcp47: "ro-RO",
        native_name: "`Română`",
        english_name: "`Romanian`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "ru",
        bcp47: "ru-RU",
        native_name: "`Русский`",
        english_name: "`Russian`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "sv",
        bcp47: "sv-SE",
        native_name: "`Svenska`",
        english_name: "`Swedish`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "th",
        bcp47: "th",
        native_name: "`ไทย`",
        english_name: "`Thai`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "tr",
        bcp47: "tr-TR",
        native_name: "`Türkçe`",
        english_name: "`Turkish`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "uk",
        bcp47: "uk",
        native_name: "`Українська`",
        english_name: "`Ukrainian`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "zh-hans",
        bcp47: "zh-CN",
        native_name: "`简体中文`",
        english_name: "`Chinese (Simplified)`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
    CmsLocaleOption {
        code: "zh-hant",
        bcp47: "zh-TW",
        native_name: "`繁體中文`",
        english_name: "`Chinese (Traditional)`",
        direction: "ltr",
        installed: false,
        translation_coverage: 0,
    },
];

/// Accept canonical codes and case-insensitive BCP-47 aliases.
pub fn find_cms_locale(raw: &str) -> Option<&'static CmsLocaleOption> {
    let trimmed = raw.trim();
    CMS_LANGUAGE_CATALOG.iter().find(|option| {
        option.code.eq_ignore_ascii_case(trimmed) || option.bcp47.eq_ignore_ascii_case(trimmed)
    })
}

/// Only installed translation packs determine the effective UI direction and language.
pub fn resolve_effective_ui_locale(configured: &str) -> &'static CmsLocaleOption {
    if configured.trim().eq_ignore_ascii_case("vi") {
        &CMS_LANGUAGE_CATALOG[1]
    } else {
        &CMS_LANGUAGE_CATALOG[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_pinned_and_normalizes_aliases() {
        assert_eq!(CMS_LANGUAGE_CATALOG.len(), 32);
        assert_eq!(CMS_LANGUAGE_CATALOG[0].code, "en");
        assert_eq!(CMS_LANGUAGE_CATALOG[1].code, "vi");
        assert!(
            CMS_LANGUAGE_CATALOG[2..]
                .windows(2)
                .all(|w| w[0].code < w[1].code)
        );
        for option in CMS_LANGUAGE_CATALOG {
            assert_eq!(find_cms_locale(option.code), Some(option));
            assert_eq!(find_cms_locale(option.bcp47), Some(option));
            assert_eq!(option.installed, matches!(option.code, "en" | "vi"));
            assert_eq!(
                option.translation_coverage,
                if option.installed { 100 } else { 0 }
            );
        }
        assert_eq!(find_cms_locale(" PT-BR ").unwrap().code, "pt-br");
        assert_eq!(find_cms_locale(" Vi-VN ").unwrap().code, "vi");
        assert_eq!(find_cms_locale("ZH-cn").unwrap().code, "zh-hans");
        assert!(find_cms_locale("unknown").is_none());
    }

    #[test]
    fn effective_locale_always_uses_an_installed_pack() {
        assert_eq!(
            resolve_effective_ui_locale(" VI "),
            &CMS_LANGUAGE_CATALOG[1]
        );
        for code in ["en", "ar", "he", "ja", "unknown", "vi-VN", ""] {
            let effective = resolve_effective_ui_locale(code);
            assert_eq!(effective.code, "en");
            assert_eq!(effective.bcp47, "en-US");
            assert_eq!(effective.direction, "ltr");
        }
    }
}
