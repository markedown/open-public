//! Minimal gettext-style internationalization.
//!
//! Translations live in standard `.po` files under `locales/`, so Poedit,
//! Weblate and similar tools work unchanged. Message ids are the English source
//! strings; each locale's `msgstr` values are filled in over time. A locale's
//! catalog is embedded at compile time and parsed once on first use; an
//! untranslated or unknown string falls back to its English source.

use std::collections::HashMap;
use std::sync::LazyLock;

/// The product name (a brand), never translated.
pub const SITE_NAME: &str = "open-public";

static TR: LazyLock<Catalog> =
    LazyLock::new(|| Catalog::parse(include_str!("../../../locales/tr.po")));

static DE: LazyLock<Catalog> =
    LazyLock::new(|| Catalog::parse(include_str!("../../../locales/de.po")));

static FR: LazyLock<Catalog> =
    LazyLock::new(|| Catalog::parse(include_str!("../../../locales/fr.po")));

/// The deployment default locale, resolved once from the `LOCALE` environment
/// variable. Used as the final fallback when a request neither carries a
/// language cookie nor a matching `Accept-Language`.
static ACTIVE: LazyLock<Lang> =
    LazyLock::new(|| Lang::from_code(std::env::var("LOCALE").as_deref().unwrap_or("tr")));

tokio::task_local! {
    /// The locale chosen for the current request. Set by the locale middleware
    /// for the duration of handling, so every `i18n::t()` call site reads it
    /// without threading a language argument through.
    static REQUEST_LANG: Lang;
    /// The absolute URL of the current request, set by the same middleware. The
    /// layout uses it as the page's canonical address without every handler
    /// having to pass its own down.
    static REQUEST_URL: String;
}

fn active() -> Lang {
    REQUEST_LANG.try_with(|l| *l).unwrap_or(*ACTIVE)
}

/// Run a request handler with `lang` as the active locale for its task.
pub async fn with_lang<F: std::future::Future>(lang: Lang, f: F) -> F::Output {
    REQUEST_LANG.scope(lang, f).await
}

/// Run a request handler with `url` recorded as the request's own address.
pub async fn with_url<F: std::future::Future>(url: String, f: F) -> F::Output {
    REQUEST_URL.scope(url, f).await
}

/// The absolute URL of the current request, when one is known. `None` outside a
/// request, and when the deployment has not been told its own origin: a
/// canonical address is worth omitting rather than guessing at.
pub fn request_url() -> Option<String> {
    REQUEST_URL
        .try_with(|u| u.clone())
        .ok()
        .filter(|u| !u.is_empty())
}

/// Resolve the request locale: an explicit `lang` cookie wins (the visitor
/// chose), then the best match from `Accept-Language` (the browser's
/// preference), then the deployment default.
pub fn resolve(cookie_lang: Option<&str>, accept_language: Option<&str>) -> Lang {
    if let Some(code) = cookie_lang {
        if let Some(lang) = Lang::known(code) {
            return lang;
        }
    }
    if let Some(header) = accept_language {
        for part in header.split(',') {
            let tag = part.split(';').next().unwrap_or("").trim();
            let primary = tag.split(['-', '_']).next().unwrap_or("");
            if let Some(lang) = Lang::known(primary) {
                return lang;
            }
        }
    }
    *ACTIVE
}

/// A supported display language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Tr,
    De,
    Fr,
}

impl Lang {
    /// BCP-47 code for the `<html lang>` attribute.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Tr => "tr",
            Lang::De => "de",
            Lang::Fr => "fr",
        }
    }

    /// Resolve a language code to a supported language; unknown codes fall back
    /// to English, the source language.
    pub fn from_code(code: &str) -> Lang {
        match code {
            "tr" => Lang::Tr,
            "de" => Lang::De,
            "fr" => Lang::Fr,
            _ => Lang::En,
        }
    }

    /// A supported language for this exact code, or `None` if unsupported.
    pub fn known(code: &str) -> Option<Lang> {
        match code {
            "en" => Some(Lang::En),
            "tr" => Some(Lang::Tr),
            "de" => Some(Lang::De),
            "fr" => Some(Lang::Fr),
            _ => None,
        }
    }

    /// Every supported language, in switcher order.
    pub const ALL: [Lang; 4] = [Lang::En, Lang::Tr, Lang::De, Lang::Fr];

    /// Whether this is the currently active locale.
    pub fn is_active(self) -> bool {
        active() == self
    }

    /// The short label shown in the language switcher.
    pub fn label(self) -> &'static str {
        match self {
            Lang::En => "EN",
            Lang::Tr => "TR",
            Lang::De => "DE",
            Lang::Fr => "FR",
        }
    }

    /// The language's own name (endonym), for the switcher menu.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Tr => "Türkçe",
            Lang::De => "Deutsch",
            Lang::Fr => "Français",
        }
    }

    /// Translate an English source string into this language, falling back to
    /// the source string when there is no translation.
    pub fn t(self, msgid: &'static str) -> &'static str {
        match self {
            Lang::En => msgid,
            Lang::Tr => TR.get(msgid),
            Lang::De => DE.get(msgid),
            Lang::Fr => FR.get(msgid),
        }
    }
}

/// Translate `msgid` in the active locale.
pub fn t(msgid: &'static str) -> &'static str {
    active().t(msgid)
}

/// The currently active language for this request.
pub fn current() -> Lang {
    active()
}

/// Translate a runtime string via the active locale's catalog, falling back to
/// the string itself. Unlike [`t`], the input need not be a compile-time
/// constant, so this suits short controlled-vocabulary values that live in the
/// database in the source language (a country's government type, for example).
/// Free-text content is not translated this way; that is tracked separately.
pub fn t_dyn(s: &str) -> &str {
    match active() {
        Lang::En => s,
        Lang::Tr => TR.get(s),
        Lang::De => DE.get(s),
        Lang::Fr => FR.get(s),
    }
}

/// A localized three-letter month abbreviation (1 = January). chrono formats
/// months only in English, so month names are localized here instead.
pub fn month_abbr(month: u32) -> &'static str {
    const EN: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    const TR: [&str; 12] = [
        "Oca", "Şub", "Mar", "Nis", "May", "Haz", "Tem", "Ağu", "Eyl", "Eki", "Kas", "Ara",
    ];
    const DE: [&str; 12] = [
        "Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez",
    ];
    const FR: [&str; 12] = [
        "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.",
        "déc.",
    ];
    let i = (month.clamp(1, 12) - 1) as usize;
    match active() {
        Lang::Tr => TR[i],
        Lang::En => EN[i],
        Lang::De => DE[i],
        Lang::Fr => FR[i],
    }
}

/// The active locale's `<html lang>` code.
pub fn lang_code() -> &'static str {
    active().code()
}

/// A parsed gettext catalog: `msgid` -> `msgstr` for one locale.
pub struct Catalog {
    messages: HashMap<String, String>,
}

impl Catalog {
    /// Parse the text of a gettext `.po` file.
    ///
    /// Handles comments, multi-line quoted strings and the standard escape
    /// sequences. The header entry (empty msgid) and entries with an empty
    /// msgstr are skipped, so those strings fall back to their msgid.
    pub fn parse(src: &str) -> Self {
        let mut messages = HashMap::new();
        let mut msgid: Option<String> = None;
        let mut msgstr: Option<String> = None;
        let mut field = Field::None;

        for raw in src.lines() {
            let line = raw.trim();
            if line.is_empty() {
                flush(&mut messages, &mut msgid, &mut msgstr);
                field = Field::None;
            } else if line.starts_with('#') {
                continue;
            } else if let Some(rest) = line.strip_prefix("msgid ") {
                flush(&mut messages, &mut msgid, &mut msgstr);
                msgid = Some(unquote(rest));
                field = Field::Id;
            } else if let Some(rest) = line.strip_prefix("msgstr ") {
                msgstr = Some(unquote(rest));
                field = Field::Str;
            } else if line.starts_with('"') {
                let piece = unquote(line);
                match field {
                    Field::Id => msgid.get_or_insert_with(String::new).push_str(&piece),
                    Field::Str => msgstr.get_or_insert_with(String::new).push_str(&piece),
                    Field::None => {}
                }
            } else {
                // Not a keyword we track (e.g. msgctxt, msgid_plural).
                field = Field::None;
            }
        }
        flush(&mut messages, &mut msgid, &mut msgstr);

        Catalog { messages }
    }

    /// The translation for `msgid`, or `msgid` itself when untranslated.
    pub fn get<'a>(&'a self, msgid: &'a str) -> &'a str {
        self.messages
            .get(msgid)
            .map(String::as_str)
            .unwrap_or(msgid)
    }
}

enum Field {
    None,
    Id,
    Str,
}

/// Store a completed entry, dropping the header and untranslated strings.
fn flush(messages: &mut HashMap<String, String>, id: &mut Option<String>, s: &mut Option<String>) {
    if let (Some(i), Some(t)) = (id.take(), s.take()) {
        if !i.is_empty() && !t.is_empty() {
            messages.insert(i, t);
        }
    }
}

/// Strip the surrounding quotes from a `.po` string literal and unescape it.
fn unquote(s: &str) -> String {
    let s = s.trim();
    let inner = s
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(s);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{resolve, Catalog, Lang};

    #[test]
    fn resolve_prefers_cookie_then_accept_language() {
        // An explicit cookie wins over the browser preference.
        assert_eq!(resolve(Some("en"), Some("tr")), Lang::En);
        assert_eq!(resolve(Some("tr"), Some("en")), Lang::Tr);
        // An unknown cookie is ignored; the header decides.
        assert_eq!(resolve(Some("xx"), Some("tr")), Lang::Tr);
        // The first supported tag in Accept-Language is chosen, region stripped.
        assert_eq!(resolve(None, Some("tr-TR,tr;q=0.9,en;q=0.8")), Lang::Tr);
        assert_eq!(resolve(None, Some("de-DE,de;q=0.9,en;q=0.8")), Lang::De);
        assert_eq!(resolve(None, Some("fr-FR,fr;q=0.9,en;q=0.8")), Lang::Fr);
        // An unsupported first tag is skipped; the next supported one wins.
        assert_eq!(resolve(None, Some("es-ES,es;q=0.9,en;q=0.8")), Lang::En);
    }

    #[test]
    fn translates_and_falls_back() {
        let po = r#"
msgid ""
msgstr "Content-Type: text/plain; charset=UTF-8\n"

msgid "Hello"
msgstr "Merhaba"

msgid "Untranslated"
msgstr ""
"#;
        let cat = Catalog::parse(po);
        assert_eq!(cat.get("Hello"), "Merhaba");
        // empty msgstr -> falls back to the source string
        assert_eq!(cat.get("Untranslated"), "Untranslated");
        // unknown key -> falls back to itself
        assert_eq!(cat.get("Missing"), "Missing");
    }

    #[test]
    fn skips_header_entry() {
        let cat = Catalog::parse("msgid \"\"\nmsgstr \"Language: tr\\n\"\n");
        assert_eq!(cat.get(""), "");
    }

    #[test]
    fn joins_multiline_and_unescapes() {
        let po = "msgid \"a\"\nmsgstr \"\"\n\"line one\\n\"\n\"line \\\"two\\\"\"\n";
        let cat = Catalog::parse(po);
        assert_eq!(cat.get("a"), "line one\nline \"two\"");
    }

    #[test]
    fn joins_a_multiline_msgid() {
        // A msgid split across continuation lines is reassembled.
        let po = "msgid \"\"\n\"first \"\n\"second\"\nmsgstr \"birinci ikinci\"\n";
        let cat = Catalog::parse(po);
        assert_eq!(cat.get("first second"), "birinci ikinci");
    }

    #[test]
    fn ignores_msgctxt_and_other_keywords() {
        // A keyword we do not track (msgctxt, msgid_plural) resets parsing, and
        // any quoted line that follows it before a msgid/msgstr is dropped
        // rather than misattributed.
        let po = "msgctxt \"menu\"\n\"stray continuation\"\nmsgid \"Home\"\nmsgstr \"Ana sayfa\"\n";
        let cat = Catalog::parse(po);
        assert_eq!(cat.get("Home"), "Ana sayfa");
        assert_eq!(cat.get("menu"), "menu"); // never stored as a key
    }

    #[test]
    fn unescapes_every_supported_sequence() {
        // \t \r \\ a literal backslash-escaped other char, and a lone trailing
        // backslash (which is dropped).
        let po = "msgid \"k\"\nmsgstr \"a\\tb\\rc\\\\d\\qe\"\n";
        let cat = Catalog::parse(po);
        assert_eq!(cat.get("k"), "a\tb\rc\\dqe");
    }

    #[test]
    fn drops_a_dangling_trailing_backslash() {
        // A backslash with no following character is dropped rather than
        // panicking.
        let po = "msgid \"z\"\nmsgstr \"x\\\"\n";
        let cat = Catalog::parse(po);
        assert_eq!(cat.get("z"), "x");
    }

    #[test]
    fn english_is_the_identity_locale() {
        // The source locale returns msgids unchanged and reports its own code.
        assert_eq!(Lang::En.code(), "en");
        assert_eq!(Lang::En.t("Anything"), "Anything");
        assert_eq!(Lang::from_code("en"), Lang::En);
        assert_eq!(Lang::from_code("zz"), Lang::En); // unknown falls back
        assert_eq!(Lang::from_code("tr"), Lang::Tr);
    }
}
