use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub type Variables = Map<String, Value>;

/// All template paths are relative to the campaign JSON file.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Campaign {
    pub from: Option<String>,
    pub reply_to: Option<String>,
    #[serde(default)]
    pub subject: String,
    pub html: Option<std::path::PathBuf>,
    pub text: Option<std::path::PathBuf>,
    #[serde(default)]
    pub variables: Variables,
    #[serde(default)]
    pub rules: Vec<MatchRule>,
    #[serde(default)]
    pub recipients: Vec<Recipient>,
}

impl Campaign {
    pub fn load(path: &Path) -> Result<Self> {
        let source =
            fs::read_to_string(path).with_context(|| format!("无法读取配置 {}", path.display()))?;
        let mut campaign: Self = serde_json::from_str(&source)
            .with_context(|| format!("配置 JSON 无效：{}", path.display()))?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        for template in [&mut campaign.html, &mut campaign.text]
            .into_iter()
            .flatten()
        {
            *template = base.join(&*template);
        }
        Ok(campaign)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Recipient {
    Email(String),
    Personalized(RecipientDetails),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecipientDetails {
    pub email: String,
    pub name: Option<String>,
    #[serde(default)]
    pub variables: Variables,
}

impl Recipient {
    pub fn email(&self) -> &str {
        match self {
            Self::Email(email) => email,
            Self::Personalized(details) => &details.email,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MatchRule {
    /// A case-insensitive glob against the complete recipient address.
    pub r#match: String,
    pub variables: Variables,
}

pub fn load_recipients(path: &Path) -> Result<Vec<Recipient>> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("无法读取收件人文件 {}", path.display()))?;
    serde_json::from_str(&source)
        .with_context(|| format!("收件人文件必须是 JSON 数组：{}", path.display()))
}

pub fn validate_variables(variables: &Variables, source: &str) -> Result<()> {
    for key in variables.keys() {
        if ["email", "local", "domain"].contains(&key.as_str()) {
            bail!("{source} 不能覆盖内置变量 {key}");
        }
        if key.trim().is_empty() {
            bail!("{source} 的变量名不能为空");
        }
    }
    Ok(())
}
