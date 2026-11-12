//! Desktop presentation language. This module never changes device configuration.
use crate::model::{LanguagePreference, UiLanguageState, UiLocale};

pub fn locale_for_tag(tag: &str) -> UiLocale {
    if tag
        .split(['-', '_'])
        .next()
        .is_some_and(|s| s.eq_ignore_ascii_case("zh"))
    {
        UiLocale::Chinese
    } else {
        UiLocale::English
    }
}

pub fn system_locale() -> UiLocale {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
        let (mut count, mut size) = (0, 0);
        unsafe {
            if GetUserPreferredUILanguages(
                MUI_LANGUAGE_NAME,
                &mut count,
                std::ptr::null_mut(),
                &mut size,
            ) == 0
                || size == 0
            {
                return UiLocale::English;
            }
            let mut buffer = vec![0u16; size as usize];
            if GetUserPreferredUILanguages(
                MUI_LANGUAGE_NAME,
                &mut count,
                buffer.as_mut_ptr(),
                &mut size,
            ) == 0
            {
                return UiLocale::English;
            }
            let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
            locale_for_tag(&String::from_utf16_lossy(&buffer[..end]))
        }
    }
    #[cfg(not(windows))]
    UiLocale::English
}

pub fn resolve(preference: LanguagePreference, system: UiLocale) -> UiLanguageState {
    UiLanguageState {
        preference,
        locale: match preference {
            LanguagePreference::System => system,
            LanguagePreference::Chinese => UiLocale::Chinese,
            LanguagePreference::English => UiLocale::English,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn primary_language_and_manual_override() {
        for tag in [
            "zh", "zh-CN", "zh-TW", "zh-HK", "zh-Hans", "zh-Hant", "ZH_tw",
        ] {
            assert_eq!(locale_for_tag(tag), UiLocale::Chinese);
        }
        for tag in ["", "en-US", "ja-JP", "de-DE", "zho"] {
            assert_eq!(locale_for_tag(tag), UiLocale::English);
        }
        // Windows returns a multi-string; only its first member is considered.
        assert_eq!(
            locale_for_tag("en-US\0zh-CN\0".split('\0').next().unwrap()),
            UiLocale::English
        );
        assert_eq!(
            resolve(LanguagePreference::English, UiLocale::Chinese).locale,
            UiLocale::English
        );
        assert_eq!(
            resolve(LanguagePreference::Chinese, UiLocale::English).locale,
            UiLocale::Chinese
        );
    }
    #[test]
    fn localized_errors_survive_context_without_changing_diagnostics() {
        let failure = crate::model::UiFailure::new("errors.assetMissing", "素材不存在：fixture")
            .arg("id", "fixture");
        let error = anyhow::Error::new(failure).context("asset request");
        let reply = crate::model::UiError::from_error(error);
        assert_eq!(reply.message.key, "errors.assetMissing");
        assert_eq!(reply.message.args["id"], "fixture");
        assert!(reply.detail.contains("asset request"));
        assert!(reply.detail.contains("素材不存在：fixture"));
        assert!(serde_json::to_value(reply).unwrap()["message"]["args"]["id"].is_string());
    }
}
