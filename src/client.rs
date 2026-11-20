use std::time::Duration;

use anyhow::{Context, Result, bail};
use lettre::{
    Message, SmtpTransport, Transport,
    transport::smtp::authentication::{Credentials, Mechanism},
};
use serde::{Deserialize, Serialize};

use crate::template::PreparedEmail;

pub const SMTP_HOST: &str = "smtp.mx.cloudflare.net";
pub const SMTP_PORT: u16 = 465;

pub struct CloudflareSmtp {
    transport: SmtpTransport,
}

pub struct Submission {
    pub response: String,
    pub message_id: Option<String>,
}

/// Allows applications to inject a transport, including an offline test transport.
pub trait MailTransport {
    fn submit(&self, message: &Message) -> Result<Submission>;
}

impl CloudflareSmtp {
    pub fn new(token: String, timeout: Duration) -> Result<Self> {
        if token.trim().is_empty() {
            bail!("Cloudflare API 令牌不能为空");
        }
        if timeout.is_zero() {
            bail!("SMTP 超时时间必须大于 0");
        }
        // relay() enforces implicit TLS and validates the server certificate.
        // Pooling is disabled: each session submits exactly one personalized email,
        // so large batches cannot exceed Cloudflare's per-session recipient limit.
        let transport = SmtpTransport::relay(SMTP_HOST)
            .context("无法初始化 Cloudflare TLS 连接")?
            .port(SMTP_PORT)
            .credentials(Credentials::new("api_token".into(), token))
            .authentication(vec![Mechanism::Plain])
            .timeout(Some(timeout))
            .build();
        Ok(Self { transport })
    }
}

impl MailTransport for CloudflareSmtp {
    fn submit(&self, message: &Message) -> Result<Submission> {
        let response = self.transport.send(message).context("SMTP 提交失败")?;
        let text = response.message().collect::<Vec<_>>().join(" ");
        Ok(Submission {
            response: format!("{} {text}", response.code()),
            message_id: extract_message_id(&text),
        })
    }
}

fn extract_message_id(response: &str) -> Option<String> {
    let start = response.rfind('<')?;
    let end = response[start..].find('>')? + start;
    let id = &response[start + 1..end];
    (id.contains('@') && !id.chars().any(char::is_whitespace))
        .then(|| response[start..=end].to_owned())
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SendStatus {
    Accepted,
    /// Cloudflare may accept but suppress the message without assigning an ID.
    AcceptedWithoutMessageId,
    Failed,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SendResult {
    pub recipient: String,
    pub status: SendStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct SendReport {
    pub accepted: usize,
    pub accepted_without_message_id: usize,
    pub failed: usize,
    pub results: Vec<SendResult>,
}

/// Submit once per recipient; report failures and continue with the rest.
/// No automatic retries: a connection failure after DATA can mean the server
/// accepted the email, and retrying blindly could send a duplicate.
pub fn send_all(
    transport: &impl MailTransport,
    messages: &[PreparedEmail],
    mut on_result: impl FnMut(&SendResult),
) -> SendReport {
    let mut report = SendReport::default();
    for email in messages {
        let result = match transport.submit(&email.message) {
            Ok(submission) => {
                let status = if submission.message_id.is_some() {
                    report.accepted += 1;
                    SendStatus::Accepted
                } else {
                    report.accepted_without_message_id += 1;
                    SendStatus::AcceptedWithoutMessageId
                };
                SendResult {
                    recipient: email.recipient.clone(),
                    status,
                    message_id: submission.message_id,
                    response: Some(submission.response),
                    error: None,
                }
            }
            Err(error) => {
                report.failed += 1;
                SendResult {
                    recipient: email.recipient.clone(),
                    status: SendStatus::Failed,
                    message_id: None,
                    response: None,
                    error: Some(format!("{error:#}")),
                }
            }
        };
        on_result(&result);
        report.results.push(result);
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_only_provider_assigned_message_ids() {
        assert_eq!(
            extract_message_id("2.0.0 Ok <id@sender.example>"),
            Some("<id@sender.example>".into())
        );
        assert_eq!(extract_message_id("2.0.0 Ok"), None);
        assert_eq!(extract_message_id("Ok <not-an-id>"), None);
    }
}
