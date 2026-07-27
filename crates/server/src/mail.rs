use lettre::{
    transport::smtp::authentication::Credentials, AsyncSmtpTransport, AsyncTransport, Message,
    Tokio1Executor,
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
            i18n::t(
                "If you did not ask for this, ignore this message and your password stays as it is. The link expires in one hour.",
            ),
        )
        .await
    }

    /// The one path every transactional mail takes.
    ///
    /// Transactional mail is plain text. There is nothing to design here but a
    /// short line and a link, and plain text is the better choice for it: it has
    /// no remote resource to load and so cannot track when it is opened, it
    /// renders the same in every client, the recipient sees the real URL rather
    /// than a button hiding one, and it has the best chance of the inbox rather
    /// than the spam folder for a young sending domain.
    async fn send(
        &self,
        to_email: &str,
        link: &str,
        subject: &str,
        intro: &str,
        outro: &str,
    ) -> anyhow::Result<()> {
        let body = text_body(intro, link, outro);

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
                    .body(body)?;
                transport.send(email).await?;
                Ok(())
            }
        }
    }
}

/// The plain-text body of a transactional mail: the explanation, the link on its
/// own line so no client mangles it, then the closing note.
fn text_body(intro: &str, link: &str, outro: &str) -> String {
    format!("{intro}\n\n{link}\n\n{outro}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mail_is_plain_text_with_the_link_and_nothing_that_loads() {
        let body = text_body(
            "Confirm your email address to activate your account:",
            "https://open-public.test/verify?token=abc123",
            "If you did not create an account, ignore this message.",
        );

        // The link is present, once, on its own line where no client can mangle
        // it, and it is the whole of the recipient's trust decision: they see
        // the real URL, not a button hiding one.
        assert!(body.contains("\nhttps://open-public.test/verify?token=abc123\n"));
        assert_eq!(
            body.matches("open-public.test/verify?token=abc123").count(),
            1
        );

        // No markup at all: nothing to render differently per client, and
        // nothing that could fetch a resource and so report that the mail was
        // opened.
        for forbidden in ["<", ">", "href", "src=", "http://", "@import", "url("] {
            assert!(
                !body.contains(forbidden),
                "a plain-text mail must not contain `{forbidden}`"
            );
        }
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
