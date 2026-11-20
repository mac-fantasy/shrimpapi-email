//! Reusable content templates, independent of sender identities and recipients.
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::editor::{CampaignEditor, RuleRow, VariableRow};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplateContent {
    pub subject: String,
    pub html_enabled: bool,
    pub html: String,
    pub text_enabled: bool,
    pub text: String,
    pub variables: Vec<VariableRow>,
    pub rules: Vec<RuleRow>,
}

impl TemplateContent {
    pub fn capture(draft: &CampaignEditor) -> Self {
        Self {
            subject: draft.subject.clone(),
            html_enabled: draft.html_enabled,
            html: draft.html.clone(),
            text_enabled: draft.text_enabled,
            text: draft.text.clone(),
            variables: draft.variables.clone(),
            rules: draft.rules.clone(),
        }
    }

    pub fn apply(&self, draft: &mut CampaignEditor) {
        draft.subject.clone_from(&self.subject);
        draft.html_enabled = self.html_enabled;
        draft.html.clone_from(&self.html);
        draft.text_enabled = self.text_enabled;
        draft.text.clone_from(&self.text);
        draft.variables.clone_from(&self.variables);
        draft.rules.clone_from(&self.rules);
    }

    pub fn editor(&self) -> CampaignEditor {
        let mut draft = CampaignEditor::default();
        self.apply(&mut draft);
        draft
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedTemplate {
    pub id: u64,
    pub name: String,
    #[serde(default = "default_true")]
    pub favorite: bool,
    pub content: TemplateContent,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplateLibrary {
    pub templates: Vec<SavedTemplate>,
    pub startup_template: Option<u64>,
}

impl Default for TemplateLibrary {
    fn default() -> Self {
        let welcome = CampaignEditor::example().expect("embedded example is valid");
        let mut templates = vec![SavedTemplate {
            id: 1,
            name: "欢迎邮件".into(),
            favorite: true,
            content: TemplateContent::capture(&welcome),
        }];
        for (id, name, subject, body, variables) in [
            (
                2,
                "通知邮件",
                "{{name}}，{{title}}",
                "你好，{{name}}！\n\n{{message}}\n\n{{product}}",
                vec![
                    ("title", "服务通知"),
                    ("message", "我们有一条新消息要告诉你。"),
                    ("product", "ShrimpAPI"),
                ],
            ),
            (
                3,
                "活动推广",
                "{{name}}，{{product}} 为你准备了专属优惠",
                "你好，{{name}}！\n\n{{offer}}\n优惠码：{{coupon}}\n活动地址：{{url}}",
                vec![
                    ("product", "ShrimpAPI"),
                    ("offer", "感谢你的支持，领取本次活动优惠。"),
                    ("coupon", "WELCOME10"),
                    ("url", "https://yourdomain.com"),
                ],
            ),
        ] {
            templates.push(SavedTemplate { id, name: name.into(), favorite: true, content: TemplateContent {
                subject: subject.into(), text_enabled: true, text: body.into(),
                html_enabled: true,
                html: format!("<!doctype html><html lang=\"zh-CN\"><meta charset=\"utf-8\"><body style=\"font-family:sans-serif;line-height:1.8;padding:32px\">{}</body></html>", body.replace('\n', "<br>")),
                variables: variables.into_iter().map(|(key, value)| VariableRow { key: key.into(), value: value.into(), json: false }).collect(),
                ..Default::default()
            }});
        }
        Self {
            templates,
            startup_template: Some(1),
        }
    }
}

impl TemplateLibrary {
    pub fn get(&self, id: u64) -> Option<&SavedTemplate> {
        self.templates.iter().find(|template| template.id == id)
    }

    pub fn add(&mut self, name: &str, content: TemplateContent) -> Result<u64> {
        let name = name.trim();
        if name.is_empty() {
            bail!("模板名称不能为空");
        }
        let id = self
            .templates
            .iter()
            .map(|template| template.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .context("模板数量超出范围")?;
        self.templates.push(SavedTemplate {
            id,
            name: name.into(),
            favorite: true,
            content,
        });
        Ok(id)
    }

    pub fn remove(&mut self, id: u64) {
        self.templates.retain(|template| template.id != id);
        if self.startup_template == Some(id) {
            self.startup_template = None;
        }
    }

    pub fn validate(&self) -> Result<()> {
        let mut ids = std::collections::HashSet::new();
        for template in &self.templates {
            if template.id == 0 || !ids.insert(template.id) {
                bail!("模板 ID 重复或无效");
            }
            if template.name.trim().is_empty() {
                bail!("模板名称不能为空");
            }
        }
        if let Some(id) = self.startup_template
            && self.get(id).is_none()
        {
            bail!("启动模板不存在");
        }
        Ok(())
    }
}
