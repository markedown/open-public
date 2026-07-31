//! How to read the participation numbers shown on a poll. Every claim here is
//! true of the schema and the published dump: the three counts are what the poll
//! page shows, the bound is what `scripts/verify_chain.py` checks, and the limits
//! are stated rather than glossed. See docs/participation-integrity.md.

use maud::{html, Markup};

use crate::auth::AuthSession;
use crate::i18n;
use crate::ui;

pub async fn page(session: Option<AuthSession>) -> Markup {
    let content = html! {
        article class="mx-auto max-w-2xl" {
            (ui::page_header(i18n::t("How participation numbers work"), None, None))

            p class="max-w-prose text-[15px] leading-relaxed text-ink" {
                (i18n::t("Each poll shows a few plain counts of how its participation formed. They let anyone judge how a poll's numbers came about, without revealing who took part. This page explains each one and, just as importantly, what it does not show."))
            }

            (section(
                i18n::t("The three counts"),
                &[
                    i18n::t("Requested is how many accounts asked for a ballot for this poll. It is a count of accounts that took part, never a list of which ones."),
                    i18n::t("Cast is how many anonymous ballots were actually cast. A ballot cannot be linked to any account, so this counts votes, not people."),
                    i18n::t("Eligible is how many verified, unbanned accounts exist on the platform: the most any poll could possibly reach. It is a ceiling, not a figure for this poll."),
                ],
            ))

            (section(
                i18n::t("When the votes were cast"),
                &[
                    i18n::t("Each poll also shows a small chart of when its ballots were cast, from the first to the last, so the shape of its participation is visible. Votes that arrive steadily look even; a burst all at once looks like a spike. The chart appears once a poll has enough ballots to read as a shape, and anyone can recompute it from the times in the published ballots."),
                ],
            ))

            (section(
                i18n::t("The bound anyone can check"),
                &[
                    i18n::t("Cast can never exceed requested, and requested can never exceed eligible. Written out: cast is at most requested, which is at most eligible. The published data at /data/polls.json carries all three, so anyone can confirm the relationship holds for every poll."),
                    i18n::t("This is what makes manufactured participation visible rather than preventable. A poll whose requested count jumped far beyond the usual, with cast sitting right against it, is the honest tell, without anyone having to assert that it happened."),
                ],
            ))

            (section(
                i18n::t("What these numbers do not show"),
                &[
                    i18n::t("They do not prove one person, one vote. An account is not a person, and nothing here establishes that one human is behind one account. They raise the cost of manipulating a poll and make it visible; they do not make it impossible."),
                    i18n::t("Two of the counts, requested and eligible, are reported by us and cannot be recomputed from the published data, because recomputing them would need the account details we deliberately do not publish. Cast is recomputable: it is simply the number of that poll's ballots in the dump."),
                ],
            ))
        }
    };

    ui::layout::document(
        Some(i18n::t("How participation numbers work")),
        session.is_some(),
        session.as_ref().is_some_and(|s| s.is_admin),
        content,
    )
}

fn section(title: &str, paragraphs: &[&str]) -> Markup {
    html! {
        section class="mt-10" {
            h2 class="text-[13px] font-bold uppercase tracking-wider text-ink-muted" { (title) }
            @for p in paragraphs {
                p class="mt-3 max-w-prose text-[15px] leading-relaxed text-ink" { (p) }
            }
        }
    }
}
