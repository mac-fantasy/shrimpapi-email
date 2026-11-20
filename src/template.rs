use std::{collections::HashSet, fs};

use anyhow::{Context, Result, bail};
use globset::GlobBuilder;
use handlebars::Handlebars;
use lettre::{
    Message,
    message::{Mailbox, MultiPart, SinglePart, header::ContentType},
};
use serde::Serialize;
use serde_json::Value;

use crate::config::{Campaign, Recipient, Variables, validate_variables};

/// One independently addressed email, ready for SMTP or a local .eml preview.
pub struct PreparedEmail {
    pub recipient: String,
    pub subject: String,
    pub html: Option<String>,
    pub text: Option<String>,
    pub message: Message,
}

#[derive(Serialize)]
pub struct PreviewEntry<'a> {
    pub recipient: &'a str,
    pub subject: &'a str,
    pub eml_file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_file: Option<String>,
}

/// Validate and render the entire batch before opening any SMTP connection.
pub fn prepare(campaign: &Campaign) -> Result<Vec<PreparedEmail>> {
    let read_template = |path: &std::path::Path| {
        fs::read_to_string(path).with_context(|| format!("无法读取模板 {}", path.display()))
    };
    let html = campaign.html.as_deref().map(read_template).transpose()?;
    let text = campaign.text.as_deref().map(read_template).transpose()?;
    prepare_with_templates(campaign, html.as_deref(), text.as_deref())
}

/// Render templates held in memory, for example from a desktop editor.
pub fn prepare_with_templates(
    campaign: &Campaign,
    html_template: Option<&str>,
    text_template: Option<&str>,
) -> Result<Vec<PreparedEmail>> {
    let from: Mailbox = parse_mailbox(
        campaign
            .from
            .as_deref()
            .context("请在「连接设置」中填写发件人")?,
        "发件人",
    )?;
    let reply_to = campaign
        .reply_to
        .as_deref()
        .map(|value| parse_mailbox(value, "回复邮箱"))
        .transpose()?;
    if campaign.subject.trim().is_empty() {
        bail!("邮件主题不能为空，请在「邮件内容」中填写主题");
    }
    if campaign.recipients.is_empty() {
        bail!("至少需要一个收件人，请在「收件人」页面添加邮箱");
    }
    if html_template.is_none() && text_template.is_none() {
        bail!("请在「邮件内容」中启用 HTML 或纯文本正文");
    }
    validate_variables(&campaign.variables, "全局配置")?;

    let mut html_engine = Handlebars::new();
    html_engine.set_strict_mode(true);
    let mut plain_engine = Handlebars::new();
    plain_engine.set_strict_mode(true);
    plain_engine.register_escape_fn(handlebars::no_escape);
    plain_engine
        .register_template_string("subject", &campaign.subject)
        .context("主题模板语法无效")?;
    for (label, template) in [("html", html_template), ("text", text_template)] {
        if let Some(template) = template {
            let engine = if label == "html" {
                &mut html_engine
            } else {
                &mut plain_engine
            };
            engine
                .register_template_string(label, template)
                .with_context(|| format!("{label} 模板语法无效"))?;
        }
    }
    let rules = campaign
        .rules
        .iter()
        .enumerate()
        .map(|(index, rule)| {
            validate_variables(&rule.variables, &format!("匹配规则 {}", index + 1))?;
            if rule.r#match.trim().is_empty() {
                bail!("匹配规则 {} 的 match 不能为空", index + 1);
            }
            let matcher = GlobBuilder::new(&rule.r#match)
                .case_insensitive(true)
                .literal_separator(false)
                .build()
                .with_context(|| format!("邮箱匹配规则无效：{}", rule.r#match))?
                .compile_matcher();
            Ok((matcher, &rule.variables))
        })
        .collect::<Result<Vec<_>>>()?;

    let mut seen = HashSet::new();
    let mut messages = Vec::new();
    for recipient in &campaign.recipients {
        let mut mailbox = parse_mailbox(recipient.email(), "收件人")?;
        let email = mailbox.email.to_string();
        let local = mailbox.email.user().to_owned();
        let domain = mailbox.email.domain().to_owned();
        let details = match recipient {
            Recipient::Personalized(details) => Some(details),
            Recipient::Email(_) => None,
        };
        if let Some(details) = details {
            validate_variables(&details.variables, &format!("收件人 {email}"))?;
            if let Some(name) = &details.name {
                validate_header(name, "收件人姓名")?;
                mailbox.name = Some(name.clone());
            }
        }
        // Preserve the first recipient record and its variables.
        if !seen.insert(email.to_ascii_lowercase()) {
            continue;
        }

        let mut variables = Variables::new();
        variables.insert(
            "name".into(),
            Value::String(mailbox.name.clone().unwrap_or_else(|| local.clone())),
        );
        variables.extend(campaign.variables.clone());
        for (matcher, replacements) in &rules {
            if matcher.is_match(&email) {
                variables.extend((*replacements).clone());
            }
        }
        if let Some(details) = details {
            variables.extend(details.variables.clone());
            if let Some(name) = &details.name {
                variables.insert("name".into(), Value::String(name.clone()));
            }
        }
        variables.insert("email".into(), Value::String(email.clone()));
        variables.insert("local".into(), Value::String(local));
        variables.insert("domain".into(), Value::String(domain));

        let render = |engine: &Handlebars<'_>, template: &str| {
            engine
                .render(template, &variables)
                .with_context(|| format!("收件人 {email} 的 {template} 模板渲染失败"))
        };
        let subject = render(&plain_engine, "subject")?;
        validate_header(&subject, "渲染后的主题")?;
        if subject.trim().is_empty() {
            bail!("收件人 {email} 的主题渲染后为空");
        }
        let html = html_template
            .map(|_| render(&html_engine, "html"))
            .transpose()?;
        let text = text_template
            .map(|_| render(&plain_engine, "text"))
            .transpose()?;
        let mut builder = Message::builder()
            .from(from.clone())
            .to(mailbox)
            .subject(&subject);
        if let Some(reply_to) = &reply_to {
            builder = builder.reply_to(reply_to.clone());
        }
        let message = match (&text, &html) {
            (Some(text), Some(html)) => builder.multipart(
                MultiPart::alternative()
                    .singlepart(SinglePart::plain(text.clone()))
                    .singlepart(SinglePart::html(html.clone())),
            ),
            (None, Some(html)) => builder.header(ContentType::TEXT_HTML).body(html.clone()),
            (Some(text), None) => builder.header(ContentType::TEXT_PLAIN).body(text.clone()),
            (None, None) => unreachable!("body templates were validated above"),
        }
        .with_context(|| format!("无法构造发往 {email} 的邮件"))?;
        if message.formatted().len() > 25 * 1024 * 1024 {
            bail!("发往 {email} 的邮件超过 Cloudflare 的 25 MiB 限制");
        }
        messages.push(PreparedEmail {
            recipient: email,
            subject,
            html,
            text,
            message,
        });
    }
    Ok(messages)
}

fn parse_mailbox(value: &str, label: &str) -> Result<Mailbox> {
    validate_header(value, label)?;
    value
        .trim()
        .parse()
        .with_context(|| format!("{label}邮箱格式无效：{value}"))
}

fn validate_header(value: &str, label: &str) -> Result<()> {
    if value.chars().any(char::is_control) {
        bail!("{label}不能含换行或控制字符");
    }
    Ok(())
}
