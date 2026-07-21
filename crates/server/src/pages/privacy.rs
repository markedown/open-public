//! What the platform stores about a person, and what it cannot do with it.
//!
//! Every claim here is checkable against the schema and the code, and none of
//! it is aspirational: an address really is only ever a keyed hash, a vote
//! really is never updated or deleted, and results really do not sample a
//! population. If any of that changes, this page changes with it.

use maud::{html, Markup};

use crate::auth::AuthSession;
use crate::i18n;
use crate::ui;

pub async fn page(session: Option<AuthSession>) -> Markup {
    let content = html! {
        article class="mx-auto max-w-2xl" {
            (ui::page_header(i18n::t("Privacy"), None, None))

            p class="max-w-prose text-[15px] leading-relaxed text-ink" {
                (i18n::t("This page describes what the platform stores, why, and for how long. Everything on it can be checked against the published source code and the database schema, both of which are open."))
            }

            (section(
                i18n::t("Reading the site"),
                &[
                    i18n::t("Nothing is required to read anything here. There is no account needed, no tracking script, no advertising network, and no third-party analytics."),
                    i18n::t("The site sets one cookie for the language you choose. It holds a language code and nothing else."),
                ],
            ))

            (section(
                i18n::t("If you create an account"),
                &[
                    i18n::t("Your email address is never stored. What is stored is a keyed hash of it, which is what lets the platform recognise a returning address without being able to read it. The address itself is used to send a message and then discarded, so it cannot be recovered from the database, by us or by anyone who obtains a copy of it."),
                    i18n::t("Your password is stored only as an argon2 hash, which cannot be reversed into the password."),
                    i18n::t("A session is a random token. Only its hash is stored, and it expires. Signing out deletes it, and setting a new password ends every session of that account."),
                ],
            ))

            (section(
                i18n::t("If you vote in a poll"),
                &[
                    i18n::t("A vote records which account cast it, so that one account votes once in a poll. It records nothing about who you are, because the platform does not know."),
                    i18n::t("A vote is never changed and never deleted, by anyone, including administrators. There is no code path that does either. A poll that needs correcting is closed and a new one opened."),
                    i18n::t("The published participation record contains, for each vote, the poll, the option, the time, and an opaque number that is unique to you within that poll and means nothing outside it. It contains no account reference and no address hash."),
                ],
            ))

            (section(
                i18n::t("If you propose a poll"),
                &[
                    i18n::t("What you write and any image you upload are visible only to you and to administrators until the proposal is approved. An uploaded image is decoded and re-encoded before it is stored, which removes any metadata it carried, including location."),
                ],
            ))

            (section(
                i18n::t("What the platform cannot tell you"),
                &[
                    i18n::t("Poll results are not a survey. Verifying an address stops one person voting repeatedly from one account; it does not draw a sample of any population, and no result here is representative of anyone but the people who answered."),
                    i18n::t("The vote record is tamper-evident, which means it can be shown that votes were not altered after the fact. It does not prove that one person voted once, and that difference is not blurred anywhere on this site."),
                ],
            ))

            (section(
                i18n::t("Your data"),
                &[
                    i18n::t("Because an address is stored only as a hash, an account cannot be looked up by anyone reading the database, and that includes us. Write to the contact address to ask about an account, and expect to be asked to prove control of the address it was created with."),
                    i18n::t("A cast vote cannot be withdrawn. It carries no identifying information, and removing it would break the record that lets anyone verify the count."),
                ],
            ))
        }
    };

    ui::layout::document(
        Some(i18n::t("Privacy")),
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
