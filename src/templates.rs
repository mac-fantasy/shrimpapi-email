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
        let preset =
            |id, name: &str, subject: &str, html: &str, text: &str, values: &[(&str, &str)]| {
                let mut variables = vec![
                    VariableRow {
                        key: "product".into(),
                        value: "ShrimpAPI".into(),
                        json: false,
                    },
                    VariableRow {
                        key: "support_email".into(),
                        value: "support@yourdomain.com".into(),
                        json: false,
                    },
                ];
                variables.extend(values.iter().map(|(key, value)| VariableRow {
                    key: (*key).into(),
                    value: (*value).into(),
                    json: false,
                }));
                SavedTemplate {
                    id,
                    name: name.into(),
                    favorite: true,
                    content: TemplateContent {
                        subject: subject.into(),
                        html_enabled: true,
                        html: html.into(),
                        text_enabled: true,
                        text: text.into(),
                        variables,
                        rules: Vec::new(),
                    },
                }
            };
        Self {
            templates: vec![
                preset(
                    1,
                    "新用户注册",
                    "{{name}}，欢迎注册 {{product}}",
                    include_str!("../assets/templates/new-user-registration.html"),
                    include_str!("../assets/templates/new-user-registration.txt"),
                    &[
                        ("welcome_message", "感谢注册，欢迎开始使用模型 API 服务。"),
                        ("console_url", "https://yourdomain.com/console"),
                        ("docs_url", "https://yourdomain.com/docs"),
                    ],
                ),
                preset(
                    2,
                    "模型上新",
                    "{{product}} 模型上新：{{model_name}}",
                    include_str!("../assets/templates/model-launch.html"),
                    include_str!("../assets/templates/model-launch.txt"),
                    &[
                        ("model_name", "新模型名称"),
                        ("provider", "模型提供方"),
                        ("model_id", "provider/model-name"),
                        (
                            "model_summary",
                            "请填写模型能力、适用场景和本次上新的主要亮点。",
                        ),
                        ("context_window", "以模型详情页为准"),
                        ("pricing", "以控制台显示为准"),
                        ("console_url", "https://yourdomain.com/console"),
                        ("model_docs_url", "https://yourdomain.com/docs/models"),
                    ],
                ),
                preset(
                    3,
                    "服务通知",
                    "{{product}} 服务通知：{{notice_title}}",
                    include_str!("../assets/templates/service-notice.html"),
                    include_str!("../assets/templates/service-notice.txt"),
                    &[
                        ("notice_title", "计划维护通知"),
                        ("notice_type", "计划维护"),
                        ("start_time", "请填写开始时间（含时区）"),
                        ("end_time", "请填写预计结束时间（含时区）"),
                        ("affected_services", "请填写受影响的服务或模型"),
                        ("impact", "请填写对请求、延迟或可用性的具体影响。"),
                        ("action_required", "请根据业务需要安排重试或切换备用模型。"),
                        ("status_url", "https://yourdomain.com/status"),
                    ],
                ),
            ],
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
