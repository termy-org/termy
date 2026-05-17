#[cfg(target_os = "macos")]
pub(crate) const DEFAULT_UTF8_LOCALE: &str = "en_US.UTF-8";
#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) const DEFAULT_UTF8_LOCALE: &str = "C.UTF-8";

#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Utf8LocaleOverridePlan {
    None,
    LcCtypeOnly,
    LcAllAndLcCtype,
}

#[cfg(unix)]
fn locale_has_utf8_tag(locale: &str) -> bool {
    let locale = locale.trim();
    let locale = locale.split_once('@').map_or(locale, |(base, _)| base);
    let Some((_, encoding)) = locale.split_once('.') else {
        return false;
    };
    let encoding = encoding.trim();
    encoding.eq_ignore_ascii_case("utf-8") || encoding.eq_ignore_ascii_case("utf8")
}

#[cfg(unix)]
fn locale_is_c_or_posix(locale: &str) -> bool {
    matches!(locale.trim().to_ascii_lowercase().as_str(), "c" | "posix")
}

#[cfg(unix)]
fn utf8_locale_from_candidate(locale: &str) -> Option<String> {
    let locale = locale.trim();
    if locale.is_empty() || locale_is_c_or_posix(locale) {
        return None;
    }

    // Some environments expose only a charset token (for example "UTF-8")
    // in locale variables. Treat those as "use the platform UTF-8 default"
    // instead of building an invalid value like "UTF-8.UTF-8".
    if locale.eq_ignore_ascii_case("utf-8") || locale.eq_ignore_ascii_case("utf8") {
        return Some(DEFAULT_UTF8_LOCALE.to_string());
    }

    let (without_modifier, modifier) = locale
        .split_once('@')
        .map_or((locale, None), |(base, modifier)| (base, Some(modifier)));
    let base = without_modifier
        .split_once('.')
        .map_or(without_modifier, |(base, _)| base)
        .trim();
    if base.is_empty() {
        return None;
    }

    let mut utf8_locale = String::with_capacity(
        base.len() + ".UTF-8".len() + modifier.map_or(0, |value| value.len() + 1),
    );
    utf8_locale.push_str(base);
    utf8_locale.push_str(".UTF-8");
    if let Some(modifier) = modifier {
        utf8_locale.push('@');
        utf8_locale.push_str(modifier);
    }
    Some(utf8_locale)
}

#[cfg(unix)]
pub(crate) fn preferred_utf8_locale(
    lc_all: Option<&str>,
    lc_ctype: Option<&str>,
    lang: Option<&str>,
) -> String {
    [lc_all, lc_ctype, lang]
        .into_iter()
        .flatten()
        .find_map(utf8_locale_from_candidate)
        .unwrap_or_else(|| DEFAULT_UTF8_LOCALE.to_string())
}

#[cfg(unix)]
pub(crate) fn utf8_locale_override_plan(
    lc_all: Option<&str>,
    lc_ctype: Option<&str>,
    lang: Option<&str>,
) -> Utf8LocaleOverridePlan {
    // Follow POSIX locale precedence for classification decisions:
    // LC_ALL overrides LC_CTYPE and LANG; LC_CTYPE overrides LANG.
    // We therefore evaluate UTF-8 status from only the single effective locale.
    let has_utf8_locale =
        effective_locale_for_decision(lc_all, lc_ctype, lang).is_some_and(locale_has_utf8_tag);
    if has_utf8_locale {
        return Utf8LocaleOverridePlan::None;
    }

    if lc_all.is_some_and(|value| !value.trim().is_empty()) {
        Utf8LocaleOverridePlan::LcAllAndLcCtype
    } else {
        Utf8LocaleOverridePlan::LcCtypeOnly
    }
}

#[cfg(unix)]
fn effective_locale_for_decision<'a>(
    lc_all: Option<&'a str>,
    lc_ctype: Option<&'a str>,
    lang: Option<&'a str>,
) -> Option<&'a str> {
    [lc_all, lc_ctype, lang]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn preferred_utf8_locale_maps_charset_only_utf8_to_default() {
        assert_eq!(
            preferred_utf8_locale(Some("UTF-8"), None, None),
            DEFAULT_UTF8_LOCALE
        );
        assert_eq!(
            preferred_utf8_locale(Some("utf8"), None, None),
            DEFAULT_UTF8_LOCALE
        );
    }

    #[test]
    fn preferred_utf8_locale_preserves_modifier_and_converts_encoding() {
        assert_eq!(
            preferred_utf8_locale(None, Some("en_US.ISO8859-1@euro"), None),
            "en_US.UTF-8@euro"
        );
    }

    #[test]
    fn preferred_utf8_locale_falls_back_for_c_or_posix() {
        assert_eq!(
            preferred_utf8_locale(Some("C"), Some("POSIX"), Some("")),
            DEFAULT_UTF8_LOCALE
        );
    }
}
