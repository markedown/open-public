//! User poll submission: the form, the multipart handler that validates and
//! stores it as a pending submission, and a page where a submitter follows the
//! state of their own submissions. Approval happens in the admin queue.

use axum::extract::{Multipart, Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use maud::{html, Markup};

use crate::auth::AuthSession;
use crate::error::PageError;
use crate::media::{self, AssetError};
use crate::state::AppState;
use crate::{i18n, ui};

/// The poll kinds a submitter may choose (mirrors the poll service).
const KINDS: [(&str, &str); 4] = [
    ("single", "Single choice"),
    ("multi", "Multiple choice"),
    ("yesno", "Yes / No"),
    ("scale", "Rating scale"),
];

/// The submission form. Requires a signed-in (hence verified) user; a suspended
/// account has no session and is redirected to sign in.
pub async fn form(
    State(state): State<AppState>,
    session: AuthSession,
    Path(country): Path<String>,
) -> Result<Markup, PageError> {
    let c = db::country::get_by_slug(&state.pool, &country)
        .await?
        .ok_or(PageError::NotFound)?;
    Ok(submit_page(
        &c.slug,
        &c.name,
        "",
        "single",
        &[],
        None,
        session.is_admin,
    ))
}

/// One option input row: a label and an optional image, in that order so the
/// multipart parser can pair each image with the label it follows.
fn option_row_markup() -> Markup {
    html! {
        div class="grid grid-cols-1 gap-2 sm:grid-cols-2" {
            input type="text" name="option" placeholder=(i18n::t("Option"))
                class="block w-full border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent";
            input type="file" name="option_image" accept="image/png,image/jpeg,image/webp"
                class="block w-full text-sm text-ink-muted file:mr-3 file:border file:border-ink file:bg-paper file:px-3 file:py-1.5 file:text-[11px] file:font-bold file:uppercase file:tracking-wide file:text-ink";
        }
    }
}

/// HTMX fragment: one more option row for the submission form.
pub async fn option_row(session: AuthSession) -> Markup {
    let _ = session; // signed-in only
    option_row_markup()
}

/// Render the submission form, optionally with an error banner and prefilled
/// text (used when validation sends the form back). File inputs never prefill.
fn submit_page(
    country_slug: &str,
    country_name: &str,
    question: &str,
    kind: &str,
    option_labels: &[String],
    error: Option<&str>,
    is_admin: bool,
) -> Markup {
    // At least three rows to start; keep any the submitter had typed.
    let rows = option_labels.len().max(3);
    let content = html! {
        section class="mx-auto max-w-xl" {
            h1 class="font-serif text-3xl font-semibold tracking-tight text-ink" {
                (i18n::t("Propose a poll"))
            }
            p class="mt-2 max-w-prose text-sm text-ink-muted" {
                (i18n::t("Your poll is checked automatically and then by an editor before it appears. Keep it neutral and on topic."))
            }

            @if let Some(err) = error {
                p class="mt-4 border-l-2 border-accent bg-paper-raised px-3 py-2 text-sm text-ink" { (err) }
            }

            form class="mt-6 space-y-4" method="post"
                 action={"/" (country_slug) "/polls/submit"} enctype="multipart/form-data" {
                p class="text-xs font-bold uppercase tracking-widest text-ink-muted" { (country_name) }

                div {
                    label class="block text-sm font-medium text-ink" for="question" { (i18n::t("Question")) }
                    input type="text" name="question" id="question" required value=(question)
                        class="mt-1 block w-full border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent";
                }

                div {
                    label class="block text-sm font-medium text-ink" for="kind" { (i18n::t("Poll type")) }
                    select name="kind" id="kind"
                        class="mt-1 block w-full border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent" {
                        @for (value, label) in KINDS {
                            option value=(value) selected[value == kind] { (i18n::t(label)) }
                        }
                    }
                }

                div {
                    label class="block text-sm font-medium text-ink" for="question_image" {
                        (i18n::t("Question image (optional)"))
                    }
                    input type="file" name="question_image" id="question_image"
                        accept="image/png,image/jpeg,image/webp"
                        class="mt-1 block w-full text-sm text-ink-muted file:mr-3 file:border file:border-ink file:bg-paper file:px-3 file:py-1.5 file:text-[11px] file:font-bold file:uppercase file:tracking-wide file:text-ink";
                    p class="mt-1 text-xs text-ink-muted" {
                        (i18n::t("PNG, JPEG, or WebP, up to 5 MB. Images are re-encoded on upload."))
                    }
                }

                p class="pt-2 text-xs font-bold uppercase tracking-widest text-ink-muted" {
                    (i18n::t("Options"))
                }
                div id="poll-options" class="space-y-2" {
                    @for i in 0..rows {
                        div class="grid grid-cols-1 gap-2 sm:grid-cols-2" {
                            input type="text" name="option" placeholder=(i18n::t("Option"))
                                value=(option_labels.get(i).map(String::as_str).unwrap_or(""))
                                class="block w-full border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent";
                            input type="file" name="option_image" accept="image/png,image/jpeg,image/webp"
                                class="block w-full text-sm text-ink-muted file:mr-3 file:border file:border-ink file:bg-paper file:px-3 file:py-1.5 file:text-[11px] file:font-bold file:uppercase file:tracking-wide file:text-ink";
                        }
                    }
                }
                button type="button"
                    hx-get={"/" (country_slug) "/polls/submit/row"} hx-target="#poll-options" hx-swap="beforeend"
                    class="text-[11px] font-bold uppercase tracking-wide text-accent transition-colors hover:underline" {
                    "+ " (i18n::t("Add option"))
                }
                (ui::button::primary(i18n::t("Submit for review")))
            }
        }
    };
    ui::layout::document(Some(i18n::t("Propose a poll")), true, is_admin, content)
}

/// A pending option collected from the multipart body.
struct PendingOption {
    label: String,
    asset_id: Option<i64>,
}

/// Handle a submitted poll. The body is `multipart/form-data` so it can carry
/// image files; each `option_image` is paired with the `option` label just
/// before it. On validation failure the form is re-rendered with a message.
pub async fn create(
    State(state): State<AppState>,
    session: AuthSession,
    Path(country): Path<String>,
    mut multipart: Multipart,
) -> Response {
    let c = match db::country::get_by_slug(&state.pool, &country).await {
        Ok(Some(c)) => c,
        Ok(None) => return PageError::NotFound.into_response(),
        Err(_) => return PageError::Server.into_response(),
    };

    let mut question = String::new();
    let mut kind = String::from("single");
    let mut question_asset: Option<i64> = None;
    let mut options: Vec<PendingOption> = Vec::new();
    let mut reject: Option<String> = None;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(_) => {
                return bad_request(&c, &question, &kind, &options, session.is_admin);
            }
        };
        let name = field.name().map(str::to_string);
        match name.as_deref() {
            Some("question") => question = field.text().await.unwrap_or_default(),
            Some("kind") => kind = field.text().await.unwrap_or_default(),
            Some("option") => options.push(PendingOption {
                label: field.text().await.unwrap_or_default(),
                asset_id: None,
            }),
            Some("question_image") => match read_image(&state, session.user_id, field).await {
                Ok(Some(id)) => question_asset = Some(id),
                Ok(None) => {}
                Err(AssetError::Rejected(m)) => reject = Some(m),
                Err(AssetError::Io(_)) => return PageError::Server.into_response(),
            },
            Some("option_image") => match read_image(&state, session.user_id, field).await {
                Ok(Some(id)) => {
                    if let Some(last) = options.last_mut() {
                        last.asset_id = Some(id);
                    }
                }
                Ok(None) => {}
                Err(AssetError::Rejected(m)) => reject = Some(m),
                Err(AssetError::Io(_)) => return PageError::Server.into_response(),
            },
            _ => {
                let _ = field.bytes().await; // drain any unexpected field
            }
        }
    }

    let question = question.trim().to_string();
    let kind = match kind.trim() {
        "" => "single".to_string(),
        k if KINDS.iter().any(|(v, _)| *v == k) => k.to_string(),
        _ => "single".to_string(),
    };
    let labels: Vec<String> = options.iter().map(|o| o.label.trim().to_string()).collect();

    if let Some(msg) = reject {
        return re_render(&c, &question, &kind, &labels, &msg, session.is_admin);
    }
    if question.is_empty() {
        return re_render(
            &c,
            &question,
            &kind,
            &labels,
            i18n::t("A question is required."),
            session.is_admin,
        );
    }
    let kept: Vec<db::submissions::NewSubmissionOption> = options
        .into_iter()
        .filter(|o| !o.label.trim().is_empty() || o.asset_id.is_some())
        .map(|o| db::submissions::NewSubmissionOption {
            label: o.label.trim().to_string(),
            asset_id: o.asset_id,
        })
        .collect();
    if kept.len() < 2 {
        return re_render(
            &c,
            &question,
            &kind,
            &labels,
            i18n::t("A poll needs at least two options."),
            session.is_admin,
        );
    }

    let new = db::submissions::NewSubmission {
        submitter_id: session.user_id,
        country_id: c.id,
        question: &question,
        kind: &kind,
        question_asset_id: question_asset,
    };
    if db::submissions::create(&state.pool, &new, &kept)
        .await
        .is_err()
    {
        return PageError::Server.into_response();
    }
    Redirect::to("/submissions").into_response()
}

/// Read one image field: `Ok(None)` if the field held no file (an empty input),
/// `Ok(Some(id))` once stored, or an error to surface.
async fn read_image(
    state: &AppState,
    user_id: i64,
    field: axum::extract::multipart::Field<'_>,
) -> Result<Option<i64>, AssetError> {
    let bytes = field
        .bytes()
        .await
        .map_err(|e| AssetError::Io(anyhow::anyhow!(e)))?;
    if bytes.is_empty() {
        return Ok(None);
    }
    let asset = media::store_upload(&state.pool, &state.asset_dir, user_id, bytes.to_vec()).await?;
    Ok(Some(asset.id))
}

fn re_render(
    c: &domain::models::Country,
    question: &str,
    kind: &str,
    labels: &[String],
    msg: &str,
    is_admin: bool,
) -> Response {
    submit_page(
        &c.slug,
        &c.name,
        question,
        kind,
        labels,
        Some(msg),
        is_admin,
    )
    .into_response()
}

fn bad_request(
    c: &domain::models::Country,
    question: &str,
    kind: &str,
    options: &[PendingOption],
    is_admin: bool,
) -> Response {
    let labels: Vec<String> = options.iter().map(|o| o.label.clone()).collect();
    re_render(
        c,
        question,
        kind,
        &labels,
        i18n::t("The upload could not be read. Please try again."),
        is_admin,
    )
}

/// A submitter's own submissions and the state of each.
pub async fn mine(
    State(state): State<AppState>,
    session: AuthSession,
) -> Result<Markup, PageError> {
    let subs = db::submissions::for_submitter(&state.pool, session.user_id).await?;
    let content = html! {
        section class="mx-auto max-w-2xl" {
            h1 class="font-serif text-3xl font-semibold tracking-tight text-ink" {
                (i18n::t("My submissions"))
            }
            p class="mt-2 max-w-prose text-sm text-ink-muted" {
                (i18n::t("Every poll you propose is listed here with its current state."))
            }
            @if subs.is_empty() {
                p class="mt-8 py-10 text-center text-sm text-ink-muted" {
                    (i18n::t("You have not proposed any polls yet."))
                }
            } @else {
                ul class="mt-6 space-y-3" {
                    @for s in &subs {
                        li class="border border-hairline p-4" {
                            div class="flex items-baseline justify-between gap-3" {
                                span class="text-sm font-medium text-ink" { (s.question) }
                                (status_badge(&s.status))
                            }
                            div class="mt-1 font-mono text-[11px] uppercase tracking-wide text-ink-muted" {
                                (s.country_name)
                            }
                            @if let Some(ref sha) = s.question_sha {
                                img src={"/media/" (sha)} alt="" loading="lazy"
                                    class="mt-2 max-h-32 border border-hairline object-contain";
                            }
                            @if matches!(s.status.as_str(), "approved") {
                                @if let Some(pid) = s.published_poll_id {
                                    a href={"/" (s.country_slug) "/polls"} class="mt-2 inline-block text-xs text-accent hover:underline" {
                                        (i18n::t("View published poll"))
                                    }
                                    span class="hidden" { (pid) }
                                }
                            }
                            @if let Some(ref reason) = s.ai_reason {
                                @if matches!(s.status.as_str(), "ai_rejected") {
                                    p class="mt-2 text-xs text-ink-muted" { (i18n::t("Reason")) ": " (reason) }
                                }
                            }
                            @if let Some(ref note) = s.admin_note {
                                p class="mt-2 text-xs text-ink-muted" { (i18n::t("Reason")) ": " (note) }
                            }
                        }
                    }
                }
            }
        }
    };
    Ok(ui::layout::document(
        Some(i18n::t("My submissions")),
        true,
        session.is_admin,
        content,
    ))
}

/// A small state pill for a submission.
fn status_badge(status: &str) -> Markup {
    let label = match status {
        "pending_ai" | "pending_admin" => i18n::t("Under review"),
        "approved" => i18n::t("Published"),
        _ => i18n::t("Not accepted"),
    };
    html! {
        span class="shrink-0 border border-hairline px-2 py-0.5 font-mono text-[10px] font-bold uppercase tracking-wide text-ink-muted" {
            (label)
        }
    }
}
