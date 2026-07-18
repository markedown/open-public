use maud::{html, Markup};

use crate::fmt;
use crate::i18n;
use crate::ui;

/// A sourced timeline of political events (foundings, elections, changes of
/// government). Reuses the shared timeline component; renders nothing when the
/// entity has no recorded events. When `see_all` is set the heading carries a
/// link to the full history (the caller passes a preview slice).
pub fn timeline(events: &[db::events::Event], see_all: Option<&str>) -> Markup {
    if events.is_empty() {
        return html! {};
    }
    html! {
        section class="mb-8" {
            (ui::section_header(i18n::t("Timeline"), see_all.map(ui::see_all_link)))
            (entries(events))
        }
    }
}

/// Just the timeline entries, without the section heading. The dedicated
/// history page supplies its own page title, so it uses this to avoid a second
/// heading and rule below its own.
pub fn entries(events: &[db::events::Event]) -> Markup {
    ui::timeline_entry::timeline_list(events.iter().map(|e| ui::timeline_entry::Entry {
        kind: kind_label(&e.kind).to_string(),
        title: e.title.clone(),
        subtitle: String::new(),
        date_range: fmt::date(e.happened_on),
        link_href: None,
        source_url: Some(e.source_url.clone()),
    }))
}

/// Localized label for a known event kind, falling back to the raw code so an
/// unrecognized kind still renders rather than vanishing.
fn kind_label(kind: &str) -> &str {
    match kind {
        "founding" => i18n::t("Founding"),
        "election" => i18n::t("Election"),
        "government" => i18n::t("Government"),
        "milestone" => i18n::t("Milestone"),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::timeline;

    fn ev(kind: &str) -> db::events::Event {
        db::events::Event {
            kind: kind.to_string(),
            title: format!("{kind} title"),
            happened_on: chrono::NaiveDate::from_ymd_opt(2023, 5, 14),
            source_url: "https://example.test/e".to_string(),
        }
    }

    #[test]
    fn empty_timeline_renders_nothing() {
        assert_eq!(timeline(&[], None).into_string(), "");
    }

    #[test]
    fn renders_known_kinds_and_falls_back_for_unknown() {
        let events = [
            ev("founding"),
            ev("election"),
            ev("government"),
            ev("milestone"),
            ev("resignation"), // unknown kind: label falls back to the raw code
        ];
        let html = timeline(&events, None).into_string();
        assert!(html.contains("founding title"));
        assert!(html.contains("resignation")); // fallback label survives
        assert!(html.contains("example.test/e")); // source marker present
    }
}
