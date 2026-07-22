use lettre::{
    message::MultiPart, transport::smtp::authentication::Credentials, AsyncSmtpTransport,
    AsyncTransport, Message, Tokio1Executor,
};

use crate::config::MailTransport;
use crate::i18n;

/// Sends transactional mail. Cloned freely into request handlers; the SMTP
/// transport keeps an internal connection pool.
#[derive(Clone)]
pub struct Mailer {
    from: String,
    base_url: String,
    kind: Kind,
}

#[derive(Clone)]
enum Kind {
    /// Log the link instead of sending. No plaintext address is stored, only logged in dev.
    Console,
    Smtp(AsyncSmtpTransport<Tokio1Executor>),
}

impl Mailer {
    pub fn new(transport: &MailTransport, from: String, base_url: String) -> anyhow::Result<Self> {
        let kind = match transport {
            MailTransport::Console => Kind::Console,
            MailTransport::Smtp {
                host,
                port,
                user,
                pass,
            } => {
                let mut builder =
                    AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)?.port(*port);
                if !user.is_empty() {
                    builder = builder.credentials(Credentials::new(user.clone(), pass.clone()));
                }
                Kind::Smtp(builder.build())
            }
        };
        Ok(Self {
            from,
            base_url,
            kind,
        })
    }

    /// Send a verification link. The address is used here and then discarded;
    /// only its HMAC hash is persisted elsewhere.
    pub async fn send_verification(&self, to_email: &str, token: &str) -> anyhow::Result<()> {
        self.send(
            to_email,
            &format!("{}/verify?token={}", self.base_url, token),
            i18n::t("Verify your email"),
            i18n::t("Confirm your email address to activate your account:"),
            i18n::t("Verify email"),
            i18n::t(
                "If you did not create an account, ignore this message. The link expires in 24 hours.",
            ),
        )
        .await
    }

    /// Send a link that sets a new password. Same shape as verification: the
    /// address is used to send and then discarded.
    pub async fn send_password_reset(&self, to_email: &str, token: &str) -> anyhow::Result<()> {
        self.send(
            to_email,
            &format!("{}/reset?token={}", self.base_url, token),
            i18n::t("Set a new password"),
            i18n::t("Use this link to set a new password:"),
            i18n::t("Set a new password"),
            i18n::t(
                "If you did not ask for this, ignore this message and your password stays as it is. The link expires in one hour.",
            ),
        )
        .await
    }

    /// The one path every transactional mail takes.
    ///
    /// A mail is HTML and a plain-text alternative carrying the same content, so
    /// a client that shows either sees the whole message. Nothing in the HTML
    /// loads from the network: no remote image, no tracking pixel, no web font.
    /// The link is a plain anchor and appears as text as well, so a client that
    /// strips the button still shows where it goes. There is no open or click
    /// tracking, here or at the provider.
    async fn send(
        &self,
        to_email: &str,
        link: &str,
        subject: &str,
        intro: &str,
        action: &str,
        outro: &str,
    ) -> anyhow::Result<()> {
        let text = format!("{intro}\n\n{link}\n\n{outro}\n");

        match &self.kind {
            Kind::Console => {
                tracing::info!(target: "mail", %link, "mail link (console transport, not sent)");
                Ok(())
            }
            Kind::Smtp(transport) => {
                let email = Message::builder()
                    .from(self.from.parse()?)
                    .to(to_email.parse()?)
                    .subject(subject)
                    .multipart(MultiPart::alternative_plain_html(
                        text,
                        html_mail(intro, link, action, outro),
                    ))?;
                transport.send(email).await?;
                Ok(())
            }
        }
    }
}

/// The HTML body of a transactional mail: entirely self-contained, every style
/// inline, no image, no font, no script, nothing fetched when it is opened.
///
/// Table-based and inline-styled on purpose. Mail clients are not browsers: they
/// strip `<style>` blocks, ignore most external CSS, and lay out with tables, so
/// this is the shape that renders the same in a webmail and in a desktop client.
fn html_mail(intro: &str, link: &str, action: &str, outro: &str) -> String {
    let intro = escape(intro);
    let action = escape(action);
    let outro = escape(outro);
    let href = escape(link);
    // Ink on paper, a solid block for the mark, monospace for the wordmark: the
    // same identity as the site, built from characters so it needs no image.
    format!(
        r#"<!doctype html>
<html lang="{lang}"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1"></head>
<body style="margin:0;padding:0;background:#f0f1f3;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:#f0f1f3;">
<tr><td align="center" style="padding:32px 16px;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="max-width:520px;background:#ffffff;border:1px solid #d9dbe0;">
<tr><td style="padding:24px 28px;border-bottom:1px solid #eceef1;">
<span style="display:inline-block;width:14px;height:14px;background:#222426;vertical-align:middle;"></span>
<span style="font-family:'IBM Plex Mono',ui-monospace,SFMono-Regular,Menlo,monospace;font-size:15px;font-weight:700;color:#222426;letter-spacing:-0.01em;vertical-align:middle;padding-left:8px;">open-public</span>
</td></tr>
<tr><td style="padding:28px;">
<p style="margin:0 0 20px;font-family:'Public Sans',Helvetica,Arial,sans-serif;font-size:15px;line-height:1.6;color:#222426;">{intro}</p>
<table role="presentation" cellpadding="0" cellspacing="0"><tr><td style="background:#222426;">
<a href="{href}" style="display:inline-block;padding:12px 22px;font-family:'Public Sans',Helvetica,Arial,sans-serif;font-size:14px;font-weight:600;color:#ffffff;text-decoration:none;">{action}</a>
</td></tr></table>
<p style="margin:20px 0 0;font-family:'IBM Plex Mono',ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;line-height:1.5;color:#5b5e63;word-break:break-all;">{href}</p>
</td></tr>
<tr><td style="padding:20px 28px;border-top:1px solid #eceef1;">
<p style="margin:0;font-family:'Public Sans',Helvetica,Arial,sans-serif;font-size:12px;line-height:1.5;color:#7a7d82;">{outro}</p>
</td></tr>
</table></td></tr></table></body></html>"#,
        lang = i18n::lang_code(),
    )
}

/// Escape the five characters that would otherwise break out of HTML text or an
/// attribute. The content here is our own copy and our own link, but escaping is
/// cheap and keeps a stray character from ever mangling the markup.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_mail_is_self_contained_and_untracked() {
        let html = html_mail(
            "Confirm your email address to activate your account:",
            "https://open-public.test/verify?token=abc123",
            "Verify email",
            "If you did not create an account, ignore this message.",
        );

        // The link is present as a real anchor and as text, so a client that
        // strips the button still shows where it goes.
        assert!(html.contains("href=\"https://open-public.test/verify?token=abc123\""));
        assert_eq!(
            html.matches("open-public.test/verify?token=abc123").count(),
            2,
            "the link appears as a button and as readable text"
        );

        // Nothing is fetched when the mail is opened: no remote resource, no
        // pixel, no font, no script. This is the rule, checked.
        for forbidden in [
            "http://", "src=", "<img", "<script", "<link", "@import", "url(",
        ] {
            assert!(
                !html.contains(forbidden),
                "a self-contained mail must not contain `{forbidden}`"
            );
        }
        // Every https:// in the body is the link itself, never a loaded asset.
        assert_eq!(html.matches("https://").count(), 2);
    }

    #[test]
    fn html_escaping_closes_the_obvious_holes() {
        let out = escape("a & b <c> \"d\" 'e'");
        assert_eq!(out, "a &amp; b &lt;c&gt; &quot;d&quot; &#39;e&#39;");
    }

    #[test]
    fn builds_both_transports() {
        assert!(Mailer::new(
            &MailTransport::Console,
            "noreply@x.test".to_string(),
            "http://x.test".to_string(),
        )
        .is_ok());
        assert!(Mailer::new(
            &MailTransport::Smtp {
                host: "smtp.x.test".to_string(),
                port: 587,
                user: "user".to_string(),
                pass: "pass".to_string(),
            },
            "noreply@x.test".to_string(),
            "http://x.test".to_string(),
        )
        .is_ok());
    }

    #[tokio::test]
    async fn console_transport_does_not_error() {
        let mailer = Mailer::new(
            &MailTransport::Console,
            "noreply@x.test".to_string(),
            "http://x.test".to_string(),
        )
        .unwrap();
        assert!(mailer.send_verification("who@x.test", "tok").await.is_ok());
    }

    #[tokio::test]
    async fn smtp_transport_builds_and_attempts_delivery() {
        // Point the SMTP transport at a closed local port: the message is built
        // and delivery is attempted, exercising the SMTP branch. The connection
        // is refused, so the call returns an error rather than sending.
        let mailer = Mailer::new(
            &MailTransport::Smtp {
                host: "127.0.0.1".to_string(),
                port: 59_999,
                user: String::new(),
                pass: String::new(),
            },
            "noreply@x.test".to_string(),
            "http://x.test".to_string(),
        )
        .unwrap();
        let result = mailer.send_verification("who@x.test", "tok").await;
        assert!(result.is_err(), "delivery to a closed port must fail");
    }

    #[tokio::test]
    async fn smtp_transport_rejects_an_unparseable_recipient() {
        // A malformed recipient address fails at message construction, before
        // any network call.
        let mailer = Mailer::new(
            &MailTransport::Smtp {
                host: "127.0.0.1".to_string(),
                port: 59_999,
                user: "u".to_string(),
                pass: "p".to_string(),
            },
            "noreply@x.test".to_string(),
            "http://x.test".to_string(),
        )
        .unwrap();
        assert!(mailer
            .send_verification("not a valid address", "tok")
            .await
            .is_err());
    }
}
