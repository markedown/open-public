use axum::{
    extract::{Query, State},
    response::{IntoResponse, Redirect, Response},
    Form,
};
use axum_extra::extract::cookie::CookieJar;
use maud::{html, Markup};
use serde::Deserialize;

use crate::auth;
use crate::i18n;
use crate::state::AppState;
use crate::ui;

const VERIFICATION_TTL_HOURS: i64 = 24;
/// A reset link is short-lived: it is the one credential that arrives by mail
/// and can change a password, so it should not sit usable in an inbox for long.
const RESET_TTL_HOURS: i64 = 1;

#[derive(Deserialize)]
pub struct Credentials {
    email: String,
    password: String,
}

#[derive(Deserialize)]
pub struct VerifyQuery {
    token: String,
}

pub async fn register_form() -> Markup {
    register_page(None)
}

pub async fn register_submit(
    State(state): State<AppState>,
    Form(form): Form<Credentials>,
) -> Result<Response, Markup> {
    let email = form.email.trim();
    if !email.contains('@') || email.len() > 254 {
        return Err(register_page(Some(i18n::t("Enter a valid email address."))));
    }
    if form.password.chars().count() < 8 {
        return Err(register_page(Some(i18n::t(
            "Password must be at least 8 characters.",
        ))));
    }

    let email_hash = auth::hash_email(email, &state.secret)
        .ok_or_else(|| register_page(Some(i18n::t("Something went wrong. Please try again."))))?;
    let password_hash = auth::hash_password(&form.password)
        .map_err(|_| register_page(Some(i18n::t("Something went wrong. Please try again."))))?;

    match db::users::insert(&state.pool, &email_hash, &password_hash).await {
        Ok(user_id) => {
            // Failures here are logged but not surfaced: the response must not
            // reveal whether this email was new.
            if let Err(e) = send_verification(&state, user_id, &email_hash, email).await {
                tracing::error!(?e, "sending verification mail failed");
            }
        }
        // Email already registered. Return the same page, without sending, so the
        // response cannot be used to tell whether an address has an account.
        Err(db::Error::UniqueViolation) => {}
        Err(e) => {
            tracing::error!(?e, "registration failed");
            return Err(register_page(Some(i18n::t(
                "Something went wrong. Please try again.",
            ))));
        }
    }

    Ok(check_email_page().into_response())
}

async fn send_verification(
    state: &AppState,
    user_id: i64,
    email_hash: &str,
    email: &str,
) -> anyhow::Result<()> {
    let token = auth::generate_session_token();
    let code_hash = auth::hash_token(&token);
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(VERIFICATION_TTL_HOURS);
    db::email_verifications::create(&state.pool, user_id, email_hash, &code_hash, expires_at)
        .await?;
    state.mailer.send_verification(email, &token).await?;
    Ok(())
}

pub async fn verify(State(state): State<AppState>, Query(query): Query<VerifyQuery>) -> Markup {
    let code_hash = auth::hash_token(&query.token);
    match db::email_verifications::consume_and_verify(&state.pool, &code_hash).await {
        Ok(Some(_)) => verified_page(),
        Ok(None) => notice_page(
            "Verify your email",
            i18n::t("This verification link is invalid or has expired."),
        ),
        Err(e) => {
            tracing::error!(?e, "verification failed");
            notice_page(
                "Verify your email",
                i18n::t("Something went wrong. Please try again."),
            )
        }
    }
}

pub async fn login_form() -> Markup {
    login_page(None)
}

pub async fn login_submit(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<Credentials>,
) -> Result<Response, Markup> {
    let email_hash = auth::hash_email(form.email.trim(), &state.secret)
        .ok_or_else(|| login_page(Some(i18n::t("Something went wrong. Please try again."))))?;

    let user = db::users::get_by_email_hash(&state.pool, &email_hash)
        .await
        .map_err(|_| login_page(Some(i18n::t("Something went wrong. Please try again."))))?;

    // Fetch the stored hash only if the account exists, but always run one argon2
    // verification so response time does not reveal whether the account exists.
    let stored = match &user {
        Some(u) => db::users::password_hash(&state.pool, u.id)
            .await
            .map_err(|_| login_page(Some(i18n::t("Something went wrong. Please try again."))))?,
        None => None,
    };
    let valid = auth::verify_password_or_dummy(&form.password, stored.as_deref());

    let user = match (valid, user) {
        (true, Some(u)) => u,
        _ => return Err(login_page(Some(i18n::t("Invalid email or password.")))),
    };

    if user.verified_at.is_none() {
        return Err(login_page(Some(i18n::t(
            "Please verify your email before logging in.",
        ))));
    }

    // A suspended account cannot sign back in. Its live sessions are already
    // invalidated at the session lookup, so this closes the re-login path too.
    if user.banned_at.is_some() {
        return Err(login_page(Some(i18n::t(
            "This account has been suspended.",
        ))));
    }

    let token = auth::start_session(&state.pool, user.id)
        .await
        .map_err(|_| login_page(Some(i18n::t("Something went wrong. Please try again."))))?;

    let jar = jar.add(auth::session_cookie(token, state.cookie_secure));
    Ok((jar, Redirect::to("/")).into_response())
}

pub async fn logout(
    session: auth::AuthSession,
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Response, Markup> {
    auth::end_session(&state.pool, session.session_id)
        .await
        .map_err(|_| {
            ui::layout::document(
                Some("Error"),
                false,
                false,
                html! { p { (i18n::t("Something went wrong. Please try again.")) } },
            )
        })?;

    let jar = jar.add(auth::clear_session_cookie());
    Ok((jar, Redirect::to("/")).into_response())
}

fn register_page(message: Option<&str>) -> Markup {
    auth_form_page("Register", "/register", false, message)
}

fn login_page(message: Option<&str>) -> Markup {
    auth_form_page("Log in", "/login", true, message)
}

/// Shared register/login form. `login` toggles the copy and drops the client-side
/// minimum-length hint (the server still enforces it on registration).
fn auth_form_page(title: &'static str, action: &str, login: bool, message: Option<&str>) -> Markup {
    ui::layout::document(
        Some(title),
        false,
        false,
        html! {
            section class="mx-auto max-w-md" {
                h1 class="text-3xl font-bold tracking-tight text-ink" {
                    (i18n::t(title))
                }
                @if let Some(msg) = message {
                    p class="mt-3 text-sm text-red-600" { (msg) }
                }
                form class="mt-6 space-y-4" method="post" action=(action) {
                    div {
                        label class="block text-sm font-medium text-ink" for="email" {
                            (i18n::t("Email"))
                        }
                        input
                            type="email" name="email" id="email" required autocomplete="email"
                            class="mt-1 block w-full rounded-lg border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent";
                    }
                    div {
                        label class="block text-sm font-medium text-ink" for="password" {
                            (i18n::t("Password"))
                        }
                        @if login {
                            input
                                type="password" name="password" id="password" required
                                autocomplete="current-password"
                                class="mt-1 block w-full rounded-lg border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent";
                        } @else {
                            input
                                type="password" name="password" id="password" required minlength="8"
                                autocomplete="new-password"
                                class="mt-1 block w-full rounded-lg border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent";
                        }
                    }
                    (ui::button::primary(i18n::t(title)))
                }
                @if login {
                    p class="mt-4 text-sm text-ink-muted" {
                        a href="/forgot" class="text-accent hover:underline" {
                            (i18n::t("Forgotten your password?"))
                        }
                    }
                    p class="mt-2 text-sm text-ink-muted" {
                        (i18n::t("No account?"))
                        " "
                        a href="/register" class="text-accent hover:underline" {
                            (i18n::t("Register"))
                        }
                    }
                } @else {
                    p class="mt-4 text-sm text-ink-muted" {
                        (i18n::t("Already have an account?"))
                        " "
                        a href="/login" class="text-accent hover:underline" {
                            (i18n::t("Log in"))
                        }
                    }
                }
            }
        },
    )
}

fn check_email_page() -> Markup {
    notice_page(
        "Register",
        i18n::t("Check your email for a link to verify your account and finish signing up."),
    )
}

fn verified_page() -> Markup {
    ui::layout::document(
        Some("Verify your email"),
        false,
        false,
        html! {
            section class="mx-auto max-w-md" {
                h1 class="text-3xl font-bold tracking-tight text-ink" {
                    (i18n::t("Email verified"))
                }
                p class="mt-3 text-sm text-ink-muted" {
                    (i18n::t("Your account is active. You can log in now."))
                }
                div class="mt-6" {
                    (ui::button::link("/login", i18n::t("Log in")))
                }
            }
        },
    )
}

fn notice_page(title: &'static str, body: &str) -> Markup {
    ui::layout::document(
        Some(title),
        false,
        false,
        html! {
            section class="mx-auto max-w-md" {
                h1 class="text-3xl font-bold tracking-tight text-ink" {
                    (i18n::t(title))
                }
                p class="mt-3 text-sm text-ink-muted" { (body) }
            }
        },
    )
}

#[derive(Deserialize)]
pub struct EmailOnly {
    email: String,
}

#[derive(Deserialize)]
pub struct ResetForm {
    token: String,
    password: String,
}

pub async fn forgot_form() -> Markup {
    forgot_page(None)
}

/// POST `/forgot`: mail a reset link if that address has an account.
///
/// The answer is the same either way. A different page, or a different response
/// time, would turn this form into a way to ask whether someone has an account
/// here, which for a political platform is a question worth refusing.
pub async fn forgot_submit(
    State(state): State<AppState>,
    Form(form): Form<EmailOnly>,
) -> Result<Response, Markup> {
    let email = form.email.trim();
    // Anything that goes wrong here is treated as "no account": the answer is
    // the same either way, so there is nothing to report and nothing a caller
    // could learn from the difference.
    let account = match auth::hash_email(email, &state.secret) {
        Some(hash) => db::users::get_by_email_hash(&state.pool, &hash).await,
        None => Ok(None),
    };

    match account {
        // An unverified account is left alone: finishing registration is the
        // path there, and a reset would otherwise verify it by the back door.
        Ok(Some(user)) if user.verified_at.is_some() && user.banned_at.is_none() => {
            if let Err(e) = send_reset(&state, user.id, email).await {
                tracing::error!(?e, "sending the reset mail failed");
            }
        }
        Ok(_) => {}
        Err(e) => tracing::error!(?e, "looking up the account failed"),
    }

    Ok(notice_page(
        "Set a new password",
        i18n::t("If that address has an account, a link to set a new password is on its way."),
    )
    .into_response())
}

async fn send_reset(state: &AppState, user_id: i64, email: &str) -> anyhow::Result<()> {
    let token = auth::generate_session_token();
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(RESET_TTL_HOURS);
    db::password_resets::create(&state.pool, user_id, &auth::hash_token(&token), expires_at)
        .await?;
    state.mailer.send_password_reset(email, &token).await?;
    Ok(())
}

/// GET `/reset`: the form that sets a new password, if the link is still good.
pub async fn reset_form(State(state): State<AppState>, Query(query): Query<VerifyQuery>) -> Markup {
    match db::password_resets::user_for_token(&state.pool, &auth::hash_token(&query.token)).await {
        Ok(Some(_)) => reset_page(&query.token, None),
        Ok(None) => notice_page(
            "Set a new password",
            i18n::t("This link is invalid or has expired."),
        ),
        Err(e) => {
            tracing::error!(?e, "checking the reset link failed");
            notice_page(
                "Set a new password",
                i18n::t("Something went wrong. Please try again."),
            )
        }
    }
}

/// POST `/reset`: set the password, spend the link, and end every session of
/// that account, since whoever asked may be locking someone else out.
pub async fn reset_submit(
    State(state): State<AppState>,
    Form(form): Form<ResetForm>,
) -> Result<Response, Markup> {
    if form.password.chars().count() < 8 {
        return Err(reset_page(
            &form.token,
            Some(i18n::t("Password must be at least 8 characters.")),
        ));
    }
    let Ok(password_hash) = auth::hash_password(&form.password) else {
        return Err(reset_page(
            &form.token,
            Some(i18n::t("Something went wrong. Please try again.")),
        ));
    };

    match db::password_resets::consume(&state.pool, &auth::hash_token(&form.token), &password_hash)
        .await
    {
        Ok(true) => Ok(notice_page(
            "Set a new password",
            i18n::t("Your password has been changed. You can log in with it now."),
        )
        .into_response()),
        Ok(false) => Ok(notice_page(
            "Set a new password",
            i18n::t("This link is invalid or has expired."),
        )
        .into_response()),
        Err(e) => {
            tracing::error!(?e, "setting the new password failed");
            Err(reset_page(
                &form.token,
                Some(i18n::t("Something went wrong. Please try again.")),
            ))
        }
    }
}

fn forgot_page(message: Option<&str>) -> Markup {
    ui::layout::document(
        Some("Set a new password"),
        false,
        false,
        html! {
            section class="mx-auto max-w-md" {
                h1 class="text-3xl font-bold tracking-tight text-ink" {
                    (i18n::t("Set a new password"))
                }
                p class="mt-3 max-w-prose text-sm text-ink-muted" {
                    (i18n::t("Enter the address you registered with and we will send a link."))
                }
                @if let Some(msg) = message {
                    p class="mt-3 text-sm text-red-600" { (msg) }
                }
                form class="mt-6 space-y-4" method="post" action="/forgot" {
                    div {
                        label class="block text-sm font-medium text-ink" for="email" {
                            (i18n::t("Email"))
                        }
                        input type="email" name="email" id="email" required autocomplete="email"
                          class="mt-1 block w-full rounded-lg border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent";
                    }
                    (ui::button::primary(i18n::t("Send the link")))
                }
                p class="mt-4 text-sm text-ink-muted" {
                    a href="/login" class="text-accent hover:underline" { (i18n::t("Back to log in")) }
                }
            }
        },
    )
}

fn reset_page(token: &str, message: Option<&str>) -> Markup {
    ui::layout::document(
        Some("Set a new password"),
        false,
        false,
        html! {
            section class="mx-auto max-w-md" {
                h1 class="text-3xl font-bold tracking-tight text-ink" {
                    (i18n::t("Set a new password"))
                }
                @if let Some(msg) = message {
                    p class="mt-3 text-sm text-red-600" { (msg) }
                }
                form class="mt-6 space-y-4" method="post" action="/reset" {
                    input type="hidden" name="token" value=(token);
                    div {
                        label class="block text-sm font-medium text-ink" for="password" {
                            (i18n::t("New password"))
                        }
                        input type="password" name="password" id="password" required minlength="8"
                          autocomplete="new-password"
                          class="mt-1 block w-full rounded-lg border border-hairline bg-paper-raised px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent";
                    }
                    (ui::button::primary(i18n::t("Set a new password")))
                }
            }
        },
    )
}
