//! The automated pre-screen for user-submitted polls.
//!
//! A [`PollReviewer`] judges a submission's text against the content policy and
//! returns a [`ReviewVerdict`]. The concrete provider is pluggable: today a
//! [`DeepSeekReviewer`] calls an OpenAI-compatible chat API, and [`DeferReviewer`]
//! (used when no provider is configured) simply routes everything to the admin
//! queue. A background [`run`] loop processes `pending_ai` submissions, so the
//! network call never sits in a user's request.
//!
//! The reviewer reads text only. Uploaded images are secured by re-encoding and
//! are reviewed by the admin; a future vision provider can screen them here
//! without changing this interface.

use std::future::Future;
use std::time::Duration;

use anyhow::Context;

use crate::config::ReviewConfig;

/// The parts of a submission the reviewer judges.
pub struct ReviewRequest {
    pub country: String,
    pub question: String,
    pub options: Vec<String>,
    pub kind: String,
}

/// What the reviewer decided about a submission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// No clear violation. The submission goes to the admin queue.
    Allow,
    /// A clear policy violation. The submission is hard-rejected.
    Reject,
}

/// A reviewer's verdict on one submission.
#[derive(Debug, Clone)]
pub struct ReviewVerdict {
    pub decision: Decision,
    pub reason: Option<String>,
    pub categories: Vec<String>,
}

/// A content-review provider. Kept generic so any model or adapter can be used.
pub trait PollReviewer: Send + Sync {
    /// An identifier for the model behind this reviewer, recorded on the review.
    fn model(&self) -> &str;

    /// Judge a submission. An `Err` is a transient failure (network, bad
    /// response), not a verdict; the caller retries and eventually defers to a
    /// human, so a reviewer outage never blocks or silently drops a submission.
    fn review(
        &self,
        req: &ReviewRequest,
    ) -> impl Future<Output = anyhow::Result<ReviewVerdict>> + Send;
}

/// The fallback used when no provider is configured: it allows every submission
/// through to the admin queue (it never auto-rejects), so a missing key degrades
/// to human-only review rather than blocking submissions.
pub struct DeferReviewer;

impl PollReviewer for DeferReviewer {
    fn model(&self) -> &str {
        "none"
    }

    async fn review(&self, _req: &ReviewRequest) -> anyhow::Result<ReviewVerdict> {
        Ok(ReviewVerdict {
            decision: Decision::Allow,
            reason: Some("no automated reviewer configured; sent for manual review".to_string()),
            categories: Vec::new(),
        })
    }
}

/// The instruction given to the model. It asks for a strict JSON verdict and to
/// hard-reject only clear violations, leaving borderline calls to a human.
const SYSTEM_PROMPT: &str = "\
You screen user-submitted political poll proposals for a neutral public-data \
platform. Judge only the text. Reject a proposal only when it clearly violates \
policy: hateful or discriminatory content, sexual or explicit content, \
violence or threats, harassment of a private individual, spam or advertising, \
anything illegal, or content that is not a genuine political poll. Do not \
reject a proposal merely for being low quality, awkwardly worded, or one-sided; \
a human reviewer handles those. Reply with a single JSON object and nothing \
else, of the form {\"decision\":\"allow\"|\"reject\",\"categories\":[\"...\"],\
\"reason\":\"one short neutral sentence\"}. Use an empty categories array when \
you allow.";

/// Build the user message: the country, the question, and the options.
fn user_message(req: &ReviewRequest) -> String {
    let mut s = format!(
        "Country: {}\nType: {}\nQuestion: {}\nOptions:\n",
        req.country, req.kind, req.question
    );
    for (i, opt) in req.options.iter().enumerate() {
        s.push_str(&format!("{}. {}\n", i + 1, opt));
    }
    s
}

/// Parse the model's JSON reply into a verdict. Tolerant of surrounding prose or
/// code fences: it reads the first `{` through the last `}`.
fn parse_verdict(content: &str) -> anyhow::Result<ReviewVerdict> {
    let start = content
        .find('{')
        .context("reviewer returned no JSON object")?;
    let end = content
        .rfind('}')
        .context("reviewer returned no JSON object")?;
    let slice = &content[start..=end];
    let v: serde_json::Value =
        serde_json::from_str(slice).context("reviewer reply was not valid JSON")?;

    let decision = match v["decision"]
        .as_str()
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("allow") => Decision::Allow,
        Some("reject") => Decision::Reject,
        _ => anyhow::bail!("reviewer returned an unknown decision"),
    };
    let categories = v["categories"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let reason = v["reason"]
        .as_str()
        .map(str::to_string)
        .filter(|s| !s.is_empty());
    Ok(ReviewVerdict {
        decision,
        reason,
        categories,
    })
}

/// A poll reviewer backed by an OpenAI-compatible chat completions API.
pub struct DeepSeekReviewer {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl DeepSeekReviewer {
    pub fn new(cfg: &ReviewConfig) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("building the reviewer HTTP client")?;
        Ok(Self {
            client,
            api_key: cfg.api_key.clone(),
            model: cfg.model.clone(),
            base_url: cfg.base_url.clone(),
        })
    }
}

impl PollReviewer for DeepSeekReviewer {
    fn model(&self) -> &str {
        &self.model
    }

    async fn review(&self, req: &ReviewRequest) -> anyhow::Result<ReviewVerdict> {
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": user_message(req)},
            ],
            "temperature": 0,
            "response_format": {"type": "json_object"},
        });
        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("calling the reviewer API")?
            .error_for_status()
            .context("reviewer API returned an error status")?;
        let data: serde_json::Value = resp.json().await.context("reading the reviewer reply")?;
        let content = data["choices"][0]["message"]["content"]
            .as_str()
            .context("reviewer reply had no content")?;
        parse_verdict(content)
    }
}

/// After this many failed automated attempts a submission is handed to the admin
/// queue, so a reviewer outage never leaves it stuck.
const MAX_AI_ATTEMPTS: i32 = 3;

/// Screen one batch of submissions awaiting the automated review. Returns how
/// many were given a verdict. Separated from [`run`] so tests can drive it once
/// with any [`PollReviewer`].
pub async fn process_pending<R: PollReviewer>(
    pool: &db::Pool,
    reviewer: &R,
    limit: i64,
) -> anyhow::Result<usize> {
    let subs = db::submissions::by_status(pool, "pending_ai", limit).await?;
    let mut done = 0;
    for s in &subs {
        let options = db::submissions::option_labels(pool, s.id).await?;
        let req = ReviewRequest {
            country: s.country_name.clone(),
            question: s.question.clone(),
            options,
            kind: s.kind.clone(),
        };
        match reviewer.review(&req).await {
            Ok(v) => {
                match v.decision {
                    Decision::Allow => {
                        db::submissions::record_ai_allow(
                            pool,
                            s.id,
                            reviewer.model(),
                            v.reason.as_deref(),
                            &v.categories,
                        )
                        .await?;
                    }
                    Decision::Reject => {
                        let banned = db::submissions::record_ai_reject(
                            pool,
                            s.id,
                            reviewer.model(),
                            v.reason.as_deref(),
                            &v.categories,
                        )
                        .await?;
                        if banned {
                            tracing::info!(
                                submission = s.id,
                                "submitter suspended after repeated violations"
                            );
                        }
                    }
                }
                done += 1;
            }
            Err(e) => {
                tracing::warn!(submission = s.id, error = %e, "poll review attempt failed");
                let attempts = db::submissions::bump_ai_attempt(pool, s.id).await?;
                if attempts >= MAX_AI_ATTEMPTS {
                    db::submissions::defer_to_admin(
                        pool,
                        s.id,
                        "automated review unavailable; sent for manual review",
                    )
                    .await?;
                }
            }
        }
    }
    Ok(done)
}

/// The background review loop: periodically screen pending submissions. Spawned
/// once at startup; it runs for the life of the process.
pub async fn run<R: PollReviewer + 'static>(pool: db::Pool, reviewer: R, interval: Duration) {
    loop {
        if let Err(e) = process_pending(&pool, &reviewer, 20).await {
            tracing::error!(error = %e, "poll review sweep failed");
        }
        tokio::time::sleep(interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_an_allow_verdict() {
        let v =
            parse_verdict(r#"{"decision":"allow","categories":[],"reason":"looks fine"}"#).unwrap();
        assert_eq!(v.decision, Decision::Allow);
        assert_eq!(v.reason.as_deref(), Some("looks fine"));
        assert!(v.categories.is_empty());
    }

    #[test]
    fn parses_a_reject_with_categories_and_prose_around_it() {
        let v = parse_verdict(
            "Here is my verdict:\n```json\n{\"decision\":\"REJECT\",\"categories\":[\"hate\",\"harassment\"],\"reason\":\"targets a group\"}\n```",
        )
        .unwrap();
        assert_eq!(v.decision, Decision::Reject);
        assert_eq!(v.categories, vec!["hate", "harassment"]);
    }

    #[test]
    fn rejects_unparseable_or_unknown_replies() {
        assert!(parse_verdict("no json here").is_err());
        assert!(parse_verdict(r#"{"decision":"maybe"}"#).is_err());
    }

    #[test]
    fn user_message_lists_every_option() {
        let msg = user_message(&ReviewRequest {
            country: "Testland".to_string(),
            question: "Q?".to_string(),
            options: vec!["A".to_string(), "B".to_string()],
            kind: "single".to_string(),
        });
        assert!(msg.contains("Country: Testland"));
        assert!(msg.contains("1. A"));
        assert!(msg.contains("2. B"));
    }
}
