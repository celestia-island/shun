//! Synthetic Source License resolution — fetches the license text and its
//! official translations from the celestia-island/sysl repository at
//! build time, so wizards show the agreement in the user's language
//! without vendoring the translations. Requires the `online` feature.

use crate::ShunError;

/// Default repository carrying the license and its translations.
pub const DEFAULT_REPO: &str = "celestia-island/sysl";
/// Default branch the license documents live on.
pub const DEFAULT_BRANCH: &str = "main";

/// Maps a BCP-47-ish locale tag onto the sysl repo's i18n directory.
pub fn sysl_dir(locale: &str) -> Option<&'static str> {
    match locale {
        "zh-Hans" | "zh" | "zh-CN" => Some("zhs"),
        "zh-Hant" | "zh-TW" | "zh-HK" => Some("zht"),
        "ja" => Some("ja"),
        "ko" => Some("ko"),
        "fr" => Some("fr"),
        "ru" => Some("ru"),
        "es" => Some("es"),
        "de" => Some("de"),
        "pt" => Some("pt"),
        "ar" => Some("ar"),
        _ => None,
    }
}

fn raw_url(repo: &str, branch: &str, path: &str) -> String {
    format!("https://raw.githubusercontent.com/{repo}/{branch}/{path}")
}

fn fetch_text(url: &str) -> Result<String, ShunError> {
    ureq::get(url)
        .call()
        .map_err(|e| ShunError::Config(format!("license fetch `{url}`: {e}")))?
        .into_string()
        .map_err(|e| ShunError::Config(format!("license read `{url}`: {e}")))
}

/// Fetches the license text for one locale — `"en"` (the default) reads
/// the root document; anything else resolves through [`sysl_dir`].
pub fn fetch_locale(repo: &str, branch: &str, locale: &str) -> Result<String, ShunError> {
    let path = match locale {
        "en" => "LICENSE.txt".to_string(),
        other => {
            let dir = sysl_dir(other).ok_or_else(|| {
                ShunError::Config(format!("no SySL translation for locale `{other}`"))
            })?;
            format!("i18n/{dir}/LICENSE.txt")
        }
    };
    fetch_text(&raw_url(repo, branch, &path))
}
