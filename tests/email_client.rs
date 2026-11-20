use std::{cell::Cell, fs, path::Path};

use anyhow::{Result, bail};
use lettre::Message;
use serde_json::json;
use shrimpapi_email::{Campaign, MailTransport, SendStatus, client::Submission, prepare, send_all};
use tempfile::TempDir;

fn fixture(html: Option<&str>, text: Option<&str>) -> (TempDir, Campaign) {
    let directory = tempfile::tempdir().unwrap();
    let mut value = json!({
        "from": "Sender <hello@sender.example>",
        "subject": "Hello {{name}}",
        "recipients": ["alice@example.com"]
    });
    if let Some(html) = html {
        fs::write(directory.path().join("body.html"), html).unwrap();
        value["html"] = "body.html".into();
    }
    if let Some(text) = text {
        fs::write(directory.path().join("body.txt"), text).unwrap();
        value["text"] = "body.txt".into();
    }
    let path = directory.path().join("campaign.json");
    fs::write(&path, value.to_string()).unwrap();
    let campaign = Campaign::load(&path).unwrap();
    (directory, campaign)
}

fn example_campaign() -> Campaign {
    Campaign::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/campaign.json")).unwrap()
}

#[test]
fn example_applies_global_domain_exact_and_recipient_variables() {
    let emails = prepare(&example_campaign()).unwrap();
    assert_eq!(emails.len(), 3);
    for (email, name, welcome, console) in [
        (
            &emails[0],
            "Alice",
            "欢迎加入团队，前往控制台开始协作。",
            "https://yourdomain.com/console/alice",
        ),
        (
            &emails[1],
            "Bob",
            "欢迎使用专业服务，如需接入帮助请联系我们。",
            "https://yourdomain.com/console/bob",
        ),
        (
            &emails[2],
            "carol",
            "感谢注册，欢迎开始使用模型 API 服务。",
            "https://yourdomain.com/console",
        ),
    ] {
        assert_eq!(email.subject, format!("{name}，欢迎注册 ShrimpAPI"));
        assert!(email.html.as_ref().unwrap().contains(welcome));
        assert!(email.text.as_ref().unwrap().contains(welcome));
        assert!(email.html.as_ref().unwrap().contains(console));
        assert!(email.text.as_ref().unwrap().contains(console));
        assert_eq!(email.message.envelope().to().len(), 1);
        assert_eq!(
            email.message.envelope().to()[0].to_string(),
            email.recipient
        );
        let raw = String::from_utf8(email.message.formatted()).unwrap();
        assert!(raw.contains("multipart/alternative"));
        assert!(raw.contains("Content-Type: text/plain; charset=utf-8"));
        assert!(raw.contains("Content-Type: text/html; charset=utf-8"));
        assert!(raw.contains("Reply-To: support@yourdomain.com"));
        for other in emails
            .iter()
            .filter(|other| other.recipient != email.recipient)
        {
            assert!(!raw.contains(&other.recipient));
        }
    }
}

#[test]
fn escapes_html_but_preserves_plain_text_subject_and_explicit_html() {
    let (_directory, mut campaign) =
        fixture(Some("{{company}} {{{snippet}}}"), Some("{{company}}"));
    campaign.subject = "{{company}}".into();
    campaign.variables =
        serde_json::from_value(json!({"company": "A&B <C>", "snippet": "<b>OK</b>"})).unwrap();
    let emails = prepare(&campaign).unwrap();
    assert_eq!(emails[0].subject, "A&B <C>");
    assert_eq!(emails[0].text.as_deref(), Some("A&B <C>"));
    assert_eq!(
        emails[0].html.as_deref(),
        Some("A&amp;B &lt;C&gt; <b>OK</b>")
    );
}

#[test]
fn matches_case_insensitively_in_order_and_protects_recipient_identity() {
    let (_directory, mut campaign) = fixture(
        None,
        Some("{{email}} {{local}} {{domain}} {{plan}} {{name}}"),
    );
    campaign.recipients = serde_json::from_value(json!([
        {"email": "Sales+Team@Example.COM", "name": "Explicit", "variables": {"name": "Ignored"}}
    ]))
    .unwrap();
    campaign.rules = serde_json::from_value(json!([
        {"match": "*@example.com", "variables": {"plan": "domain", "name": "Domain"}},
        {"match": "sales+*@example.com", "variables": {"plan": "tag"}},
        {"match": "SALES+TEAM@EXAMPLE.COM", "variables": {"plan": "exact"}}
    ]))
    .unwrap();
    let emails = prepare(&campaign).unwrap();
    assert_eq!(
        emails[0].text.as_deref(),
        Some("Sales+Team@Example.COM Sales+Team Example.COM exact Explicit")
    );
}

#[test]
fn retains_first_duplicate_and_its_personalization() {
    let (_directory, mut campaign) = fixture(None, Some("{{name}} {{plan}}"));
    campaign.recipients = serde_json::from_value(json!([
        {"email": "alice@example.com", "name": "First", "variables": {"plan": "first"}},
        {"email": "ALICE@EXAMPLE.COM", "name": "Second", "variables": {"plan": "second"}}
    ]))
    .unwrap();
    let emails = prepare(&campaign).unwrap();
    assert_eq!(emails.len(), 1);
    assert_eq!(emails[0].text.as_deref(), Some("First first"));
}

#[test]
fn missing_variable_in_later_recipient_rejects_entire_batch() {
    let (_directory, mut campaign) = fixture(Some("{{coupon}}"), None);
    campaign.recipients = serde_json::from_value(json!([
        {"email": "alice@example.com", "variables": {"coupon": "OK"}},
        "bob@example.com"
    ]))
    .unwrap();
    let error = prepare(&campaign).err().unwrap();
    let message = format!("{error:#}");
    assert!(message.contains("bob@example.com"));
    assert!(message.contains("coupon"));
}

#[test]
fn supports_nested_variables_conditions_and_lists() {
    let (_directory, mut campaign) = fixture(
        Some(
            "{{account.id}} {{#if enabled}}yes{{else}}no{{/if}} {{#each items}}[{{this}}]{{/each}}",
        ),
        None,
    );
    campaign.variables = serde_json::from_value(json!({
        "account": {"id": 42}, "enabled": false, "items": ["one", "two"]
    }))
    .unwrap();
    let emails = prepare(&campaign).unwrap();
    assert_eq!(emails[0].html.as_deref(), Some("42 no [one][two]"));
}

#[test]
fn rejects_invalid_addresses_headers_and_reserved_variables() {
    let (_directory, mut campaign) = fixture(None, Some("Hello"));
    campaign.recipients = serde_json::from_value(json!(["invalid-address"])).unwrap();
    assert!(prepare(&campaign).is_err());
    campaign.recipients = serde_json::from_value(json!(["alice@example.com"])).unwrap();
    campaign.subject = "{{injected}}".into();
    campaign
        .variables
        .insert("injected".into(), "hello\r\nBcc: eve@example.com".into());
    assert!(format!("{:#}", prepare(&campaign).err().unwrap()).contains("控制字符"));
    campaign.subject = "Hello".into();
    campaign
        .variables
        .insert("email".into(), "eve@example.com".into());
    assert!(format!("{:#}", prepare(&campaign).err().unwrap()).contains("不能覆盖"));
    campaign.variables.clear();
    campaign.recipients = serde_json::from_value(json!([
        {"email": "alice@example.com", "name": "Alice\nBcc: eve@example.com"}
    ]))
    .unwrap();
    assert!(prepare(&campaign).is_err());
}

#[test]
fn rejects_bad_rule_template_and_incomplete_campaign() {
    let (_directory, mut campaign) = fixture(None, Some("Hello"));
    campaign.rules = serde_json::from_value(json!([{"match": "[", "variables": {}}])).unwrap();
    assert!(prepare(&campaign).is_err());
    campaign.rules.clear();
    campaign.subject = "{{#if broken}}".into();
    assert!(prepare(&campaign).is_err());
    campaign.subject = "Hello".into();
    campaign.text = None;
    assert!(prepare(&campaign).is_err());
    assert!(serde_json::from_value::<Campaign>(json!({"recipents": []})).is_err());
}

#[test]
fn builds_html_only_and_text_only_messages() {
    for (html, text, content_type) in [
        (Some("<p>Hello {{name}}</p>"), None, "text/html"),
        (None, Some("Hello {{name}}"), "text/plain"),
    ] {
        let (_directory, campaign) = fixture(html, text);
        let emails = prepare(&campaign).unwrap();
        let raw = String::from_utf8(emails[0].message.formatted()).unwrap();
        assert!(raw.contains(&format!("Content-Type: {content_type}; charset=utf-8")));
        assert!(!raw.contains("multipart/alternative"));
    }
}

struct MixedTransport {
    calls: Cell<usize>,
}

impl MailTransport for MixedTransport {
    fn submit(&self, message: &Message) -> Result<Submission> {
        assert_eq!(message.envelope().to().len(), 1);
        let index = self.calls.get();
        self.calls.set(index + 1);
        match index {
            0 => Ok(Submission {
                response: "250 2.0.0 Ok <provider@sender.example>".into(),
                message_id: Some("<provider@sender.example>".into()),
            }),
            1 => bail!("550 rejected"),
            2 => Ok(Submission {
                response: "250 2.0.0 Ok".into(),
                message_id: None,
            }),
            _ => panic!("unexpected retry"),
        }
    }
}

#[test]
fn continues_after_failure_without_retry_and_distinguishes_suppressed_responses() {
    let emails = prepare(&example_campaign()).unwrap();
    let transport = MixedTransport {
        calls: Cell::new(0),
    };
    let mut seen = Vec::new();
    let report = send_all(&transport, &emails, |result| {
        seen.push(result.recipient.clone())
    });
    assert_eq!(transport.calls.get(), 3);
    assert_eq!(seen.len(), 3);
    assert_eq!(
        (
            report.accepted,
            report.failed,
            report.accepted_without_message_id
        ),
        (1, 1, 1)
    );
    assert_eq!(report.results[0].status, SendStatus::Accepted);
    assert_eq!(
        report.results[0].message_id.as_deref(),
        Some("<provider@sender.example>")
    );
    assert_eq!(report.results[1].status, SendStatus::Failed);
    assert!(report.results[1].error.as_ref().unwrap().contains("550"));
    assert_eq!(
        report.results[2].status,
        SendStatus::AcceptedWithoutMessageId
    );
    let json = serde_json::to_value(report).unwrap();
    assert_eq!(json["results"][2]["status"], "accepted_without_message_id");
    assert!(json["results"][2].get("message_id").is_none());
}
