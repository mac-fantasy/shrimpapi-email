//! Serializable campaign editing model. API credentials are never part of it.
use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    Campaign, PreparedEmail,
    config::{MatchRule, Recipient, RecipientDetails, Variables, validate_variables},
    prepare_with_templates,
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VariableRow {
    pub key: String,
    pub value: String,
    pub json: bool,
}

impl VariableRow {
    fn from_variables(variables: &Variables) -> Vec<Self> {
        variables
            .iter()
            .map(|(key, value)| Self {
                key: key.clone(),
                value: value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string()),
                json: !value.is_string(),
            })
            .collect()
    }

    fn to_variables(rows: &[Self], source: &str) -> Result<Variables> {
        let mut variables = Variables::new();
        for row in rows {
            if row.key.is_empty() && row.value.is_empty() {
                continue;
            }
            let key = row.key.trim();
            if key.is_empty() {
                bail!("{source} 的变量名不能为空");
            }
            let value = if row.json {
                serde_json::from_str(&row.value)
                    .with_context(|| format!("{source} 变量 {key} 的 JSON 值无效"))?
            } else {
                Value::String(row.value.clone())
            };
            if variables.insert(key.into(), value).is_some() {
                bail!("{source} 有重复变量：{key}");
            }
        }
        validate_variables(&variables, source)?;
        Ok(variables)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecipientRow {
    pub email: String,
    pub name: String,
    pub variables: Vec<VariableRow>,
}

impl RecipientRow {
    pub fn from_recipient(recipient: Recipient) -> Self {
        match recipient {
            Recipient::Email(email) => Self {
                email,
                ..Self::default()
            },
            Recipient::Personalized(details) => Self {
                email: details.email,
                name: details.name.unwrap_or_default(),
                variables: VariableRow::from_variables(&details.variables),
            },
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleRow {
    pub pattern: String,
    pub variables: Vec<VariableRow>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CampaignEditor {
    pub from: String,
    pub reply_to: String,
    pub subject: String,
    pub html_enabled: bool,
    pub html: String,
    pub text_enabled: bool,
    pub text: String,
    pub variables: Vec<VariableRow>,
    pub rules: Vec<RuleRow>,
    pub recipients: Vec<RecipientRow>,
}

impl CampaignEditor {
    pub fn example() -> Result<Self> {
        let campaign = serde_json::from_str(include_str!("../examples/campaign.json"))?;
        Ok(Self::from_campaign(
            campaign,
            Some(include_str!("../assets/templates/new-user-registration.html").into()),
            Some(include_str!("../assets/templates/new-user-registration.txt").into()),
        ))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let campaign = Campaign::load(path)?;
        let read = |path: &Path| {
            fs::read_to_string(path).with_context(|| format!("无法读取模板 {}", path.display()))
        };
        let html = campaign.html.as_deref().map(read).transpose()?;
        let text = campaign.text.as_deref().map(read).transpose()?;
        Ok(Self::from_campaign(campaign, html, text))
    }

    fn from_campaign(campaign: Campaign, html: Option<String>, text: Option<String>) -> Self {
        Self {
            from: campaign.from.unwrap_or_default(),
            reply_to: campaign.reply_to.unwrap_or_default(),
            subject: campaign.subject,
            html_enabled: html.is_some(),
            html: html.unwrap_or_default(),
            text_enabled: text.is_some(),
            text: text.unwrap_or_default(),
            variables: VariableRow::from_variables(&campaign.variables),
            rules: campaign
                .rules
                .into_iter()
                .map(|rule| RuleRow {
                    pattern: rule.r#match,
                    variables: VariableRow::from_variables(&rule.variables),
                })
                .collect(),
            recipients: campaign
                .recipients
                .into_iter()
                .map(RecipientRow::from_recipient)
                .collect(),
        }
    }

    pub fn campaign(&self) -> Result<Campaign> {
        let recipients = self
            .recipients
            .iter()
            .enumerate()
            .map(|(index, row)| {
                Ok(Recipient::Personalized(RecipientDetails {
                    email: row.email.clone(),
                    name: (!row.name.is_empty()).then(|| row.name.clone()),
                    variables: VariableRow::to_variables(
                        &row.variables,
                        &format!("收件人 {}", index + 1),
                    )?,
                }))
            })
            .collect::<Result<_>>()?;
        let rules = self
            .rules
            .iter()
            .enumerate()
            .map(|(index, rule)| {
                Ok(MatchRule {
                    r#match: rule.pattern.clone(),
                    variables: VariableRow::to_variables(
                        &rule.variables,
                        &format!("规则 {}", index + 1),
                    )?,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Campaign {
            from: (!self.from.is_empty()).then(|| self.from.clone()),
            reply_to: (!self.reply_to.is_empty()).then(|| self.reply_to.clone()),
            subject: self.subject.clone(),
            variables: VariableRow::to_variables(&self.variables, "全局变量")?,
            recipients,
            rules,
            ..Campaign::default()
        })
    }

    pub fn prepare(&self) -> Result<Vec<PreparedEmail>> {
        prepare_with_templates(
            &self.campaign()?,
            self.html_enabled.then_some(self.html.as_str()),
            self.text_enabled.then_some(self.text.as_str()),
        )
    }

    /// Save a campaign and adjacent template files for reopening in the app.
    pub fn save(&self, path: &Path) -> Result<()> {
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            bail!("配置文件必须使用 .json 扩展名");
        }
        let mut campaign = self.campaign()?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .context("配置文件名无效")?;
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
        for (enabled, body, extension, target) in [
            (self.html_enabled, &self.html, "html", &mut campaign.html),
            (self.text_enabled, &self.text, "txt", &mut campaign.text),
        ] {
            if enabled {
                let filename = format!("{stem}.{extension}");
                fs::write(parent.join(&filename), body)
                    .with_context(|| format!("无法保存 {extension} 模板"))?;
                *target = Some(filename.into());
            }
        }
        fs::write(path, serde_json::to_vec_pretty(&campaign)?)
            .with_context(|| format!("无法保存配置 {}", path.display()))
    }

    pub fn add_addresses(&mut self, input: &str) {
        self.recipients.extend(
            input
                .split([',', ';', '\n', '\r'])
                .map(str::trim)
                .filter(|email| !email.is_empty())
                .map(|email| RecipientRow {
                    email: email.into(),
                    ..RecipientRow::default()
                }),
        );
    }
}
