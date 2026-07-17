use lettre::{
    message::header::ContentType, transport::smtp::authentication::Credentials, AsyncSmtpTransport,
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

    /// Send a verification link to a plaintext address. The address is used here
    /// and then discarded; only its HMAC hash is persisted elsewhere.
    pub async fn send_verification(&self, to_email: &str, token: &str) -> anyhow::Result<()> {
        let link = format!("{}/verify?token={}", self.base_url, token);
        let subject = i18n::t("Verify your email");
        let intro = i18n::t("Confirm your email address to activate your account:");
        let outro = i18n::t(
            "If you did not create an account, ignore this message. The link expires in 24 hours.",
        );
        let body = format!("{intro}\n\n{link}\n\n{outro}\n");

        match &self.kind {
            Kind::Console => {
                tracing::info!(target: "mail", %link, "verification link (console transport, not sent)");
                Ok(())
            }
            Kind::Smtp(transport) => {
                let email = Message::builder()
                    .from(self.from.parse()?)
                    .to(to_email.parse()?)
                    .subject(subject)
                    .header(ContentType::TEXT_PLAIN)
                    .body(body)?;
                transport.send(email).await?;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
