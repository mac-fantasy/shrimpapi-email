use std::{
    fs,
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};

use crate::branding;
use shrimpapi_email::{
    CloudflareSmtp, PreparedEmail, SendReport, SendStatus,
    client::{SMTP_HOST, SMTP_PORT, SendResult},
    config::load_recipients,
    editor::{CampaignEditor, RecipientRow, RuleRow, VariableRow},
    output::export_previews,
    send_all,
    settings::{
        AppSettings, CredentialStore, Page, SettingsStore, SystemCredentialStore, UiSettings,
    },
    templates::{TemplateContent, TemplateLibrary},
};

const ACCENT: Color32 = Color32::from_rgb(8, 127, 140);
const INK: Color32 = Color32::from_rgb(23, 43, 65);
const MUTED: Color32 = Color32::from_rgb(103, 119, 138);
const BORDER: Color32 = Color32::from_rgb(224, 230, 237);
const BACKGROUND: Color32 = Color32::from_rgb(245, 247, 250);

pub fn run() -> Result<()> {
    let store = SettingsStore::system()?;
    let loaded = store.load()?;
    fs::create_dir_all(&store.directory)?;
    let window_path = store.window_path();
    let token = SystemCredentialStore.load();
    let icon = branding::window_icon()?;
    eframe::run_native(
        "ShrimpAPI Email",
        eframe::NativeOptions {
            persistence_path: Some(window_path),
            viewport: egui::ViewportBuilder::default()
                .with_app_id(shrimpapi_email::settings::APP_ID)
                .with_icon(icon)
                .with_inner_size([1360.0, 880.0])
                .with_min_inner_size([1040.0, 700.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            configure_style(&cc.egui_ctx);
            let mut app = EmailApp::restore(loaded.settings);
            app.store = Some(store);
            if let Some(warning) = loaded.warning {
                app.storage_status = warning;
            }
            match token {
                Ok(token) => {
                    app.saved_token.clone_from(&token);
                    app.token = token;
                }
                Err(error) => {
                    app.storage_status = format!("{error:#}；可重新输入令牌后保存。");
                    app.storage_error = true;
                }
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|error| anyhow::anyhow!("无法启动图形界面：{error}"))
}

trait PageTitle {
    fn title(self) -> &'static str;
}

impl PageTitle for Page {
    fn title(self) -> &'static str {
        match self {
            Self::Compose => "邮件内容",
            Self::Templates => "模板管理",
            Self::Recipients => "收件人",
            Self::Variables => "变量与匹配规则",
            Self::Connection => "连接设置",
            Self::Results => "发送记录",
        }
    }
}

enum Event {
    Preview(Vec<PreparedEmail>),
    Submitted(SendResult),
    Finished(SendReport),
    Error(String),
}

struct EmailApp {
    logo: Option<egui::TextureHandle>,
    draft: CampaignEditor,
    library: TemplateLibrary,
    selected_template: Option<u64>,
    store: Option<SettingsStore>,
    persisted: Option<AppSettings>,
    pending_since: Option<Instant>,
    saved_token: String,
    storage_status: String,
    storage_error: bool,
    config_path: Option<PathBuf>,
    page: Page,
    token: String,
    show_token: bool,
    timeout_secs: u64,
    bulk_addresses: String,
    preview: Vec<PreparedEmail>,
    preview_source: Option<CampaignEditor>,
    selected_recipient: usize,
    edit_html: bool,
    preview_html: bool,
    receiver: Option<mpsc::Receiver<Event>>,
    sending: bool,
    results: Vec<SendResult>,
    report: Option<SendReport>,
    status: String,
    error: bool,
    browser_files: Vec<tempfile::TempDir>,
}

impl EmailApp {
    fn new(draft: CampaignEditor) -> Self {
        let preview = draft.prepare().unwrap_or_default();
        let preview_source = (!preview.is_empty()).then(|| draft.clone());
        Self {
            logo: None,
            library: TemplateLibrary::default(),
            selected_template: Some(1),
            store: None,
            persisted: None,
            pending_since: None,
            saved_token: String::new(),
            storage_status: "设置将自动保存。".into(),
            storage_error: false,
            draft,
            config_path: None,
            page: Page::Compose,
            token: String::new(),
            show_token: false,
            timeout_secs: 30,
            bulk_addresses: String::new(),
            preview,
            preview_source,
            selected_recipient: 0,
            edit_html: true,
            preview_html: false,
            receiver: None,
            sending: false,
            results: Vec::new(),
            report: None,
            status: "已载入配置，可编辑内容或生成预览。".into(),
            error: false,
            browser_files: Vec::new(),
        }
    }

    fn restore(settings: AppSettings) -> Self {
        let mut app = Self::new(settings.draft.clone());
        app.library = settings.library.clone();
        app.timeout_secs = settings.timeout_secs;
        app.page = settings.ui.page;
        app.edit_html = settings.ui.edit_html;
        app.preview_html = settings.ui.preview_html;
        app.selected_recipient = settings
            .ui
            .selected_recipient
            .min(app.preview.len().saturating_sub(1));
        app.selected_template = settings.ui.selected_template;
        app.config_path = settings.ui.config_path.clone();
        app.bulk_addresses = settings.ui.bulk_addresses.clone();
        app.results = settings
            .last_report
            .as_ref()
            .map(|report| report.results.clone())
            .unwrap_or_default();
        app.report = settings.last_report.clone();
        app.persisted = Some(settings);
        app.status = "已恢复上次的邮件草稿、模板和设置。".into();
        app
    }

    fn snapshot(&self) -> AppSettings {
        let last_report = if self.results.is_empty() {
            self.report.clone()
        } else {
            let mut report = SendReport {
                results: self.results.clone(),
                ..Default::default()
            };
            for result in &report.results {
                match result.status {
                    SendStatus::Accepted => report.accepted += 1,
                    SendStatus::AcceptedWithoutMessageId => report.accepted_without_message_id += 1,
                    SendStatus::Failed => report.failed += 1,
                }
            }
            Some(report)
        };
        AppSettings {
            draft: self.draft.clone(),
            library: self.library.clone(),
            timeout_secs: self.timeout_secs,
            ui: UiSettings {
                page: self.page,
                edit_html: self.edit_html,
                preview_html: self.preview_html,
                selected_recipient: self.selected_recipient,
                selected_template: self.selected_template,
                config_path: self.config_path.clone(),
                bulk_addresses: self.bulk_addresses.clone(),
            },
            last_report,
            ..AppSettings::default()
        }
    }

    fn persist_with(&mut self, credentials: &impl CredentialStore) -> Result<()> {
        let Some(store) = &self.store else {
            return Ok(());
        };
        let settings = self.snapshot();
        if self.persisted.as_ref() != Some(&settings) || !store.path().exists() {
            store.save(&settings)?;
            self.persisted = Some(settings);
        }
        if self.token != self.saved_token {
            credentials.save(&self.token)?;
            self.saved_token.clone_from(&self.token);
        }
        self.storage_error = false;
        self.pending_since = None;
        self.storage_status = "设置已自动保存".into();
        Ok(())
    }

    fn persist_now(&mut self) {
        if let Err(error) = self.persist_with(&SystemCredentialStore) {
            self.storage_status = format!("设置保存失败：{error:#}");
            self.storage_error = true;
            self.pending_since = None;
        }
    }

    fn autosave(&mut self, ctx: &egui::Context) {
        if self.store.is_none() {
            return;
        }
        let changed = self.persisted.as_ref() != Some(&self.snapshot())
            || self.token != self.saved_token
            || self
                .store
                .as_ref()
                .is_some_and(|store| !store.path().exists());
        if changed && !self.storage_error {
            let since = self.pending_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= Duration::from_millis(750) {
                self.persist_now();
            } else {
                ctx.request_repaint_after(Duration::from_millis(800));
            }
        }
    }

    fn apply_template(&mut self, id: u64) {
        if let Some(template) = self.library.get(id) {
            template.content.apply(&mut self.draft);
            self.selected_template = Some(id);
            self.preview.clear();
            self.preview_source = None;
            self.selected_recipient = 0;
            self.status = format!("已应用模板「{}」，可继续编辑并生成预览。", template.name);
            self.error = false;
            self.page = Page::Compose;
        }
    }

    fn new_draft(&mut self) {
        let mut draft = CampaignEditor {
            from: self.draft.from.clone(),
            reply_to: self.draft.reply_to.clone(),
            ..Default::default()
        };
        if let Some(template) = self
            .library
            .startup_template
            .and_then(|id| self.library.get(id))
        {
            template.content.apply(&mut draft);
        }
        self.draft = draft;
        self.config_path = None;
        self.preview.clear();
        self.preview_source = None;
        self.selected_recipient = 0;
        self.page = Page::Compose;
        self.status = "已创建草稿，使用启动默认模板。".into();
    }

    fn notify(&mut self, result: Result<()>, success: &str) {
        match result {
            Ok(()) => {
                self.status = success.into();
                self.error = false;
            }
            Err(error) => {
                self.status = format!("{error:#}");
                self.error = true;
            }
        }
    }

    fn start(&mut self, ctx: &egui::Context, send: bool) {
        if self.receiver.is_some() {
            return;
        }
        if send && self.token.trim().is_empty() {
            self.page = Page::Connection;
            self.notify(Err(anyhow::anyhow!("请先填写 Cloudflare API 令牌。")), "");
            return;
        }
        let draft = self.draft.clone();
        let token = self.token.clone();
        let timeout = self.timeout_secs;
        let repaint = ctx.clone();
        let (sender, receiver) = mpsc::channel();
        let spawn = std::thread::Builder::new()
            .name("email-worker".into())
            .spawn(move || {
                let emit = |event| {
                    let _ = sender.send(event);
                    repaint.request_repaint();
                };
                let work = || -> Result<()> {
                    let emails = draft.prepare()?;
                    if send {
                        let transport = CloudflareSmtp::new(token, Duration::from_secs(timeout))?;
                        let report = send_all(&transport, &emails, |result| {
                            emit(Event::Submitted(result.clone()))
                        });
                        emit(Event::Finished(report));
                    } else {
                        emit(Event::Preview(emails));
                    }
                    Ok(())
                };
                if let Err(error) = work() {
                    emit(Event::Error(format!("{error:#}")));
                }
            });
        match spawn {
            Ok(_) => {
                self.receiver = Some(receiver);
                self.sending = send;
                self.error = false;
                if send {
                    self.results.clear();
                    self.report = None;
                    self.page = Page::Results;
                    self.status = "正在检查全部邮件，然后逐封提交…".into();
                } else {
                    self.preview.clear();
                    self.selected_recipient = 0;
                    self.preview_source = Some(self.draft.clone());
                    self.status = "正在为每个收件人生成预览…".into();
                }
            }
            Err(error) => self.notify(Err(error.into()), ""),
        }
    }

    fn poll(&mut self) {
        let events: Vec<_> = self
            .receiver
            .as_ref()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Preview(emails) => {
                    self.status = format!("已验证并生成 {} 封独立邮件，尚未发送。", emails.len());
                    self.preview = emails;
                    self.selected_recipient = 0;
                    self.receiver = None;
                }
                Event::Submitted(result) => {
                    self.results.push(result);
                    self.status = format!("已处理 {} 个收件人…", self.results.len());
                }
                Event::Finished(report) => {
                    self.status = format!(
                        "提交完成：已接受 {}，无 Message-ID {}，失败 {}。",
                        report.accepted, report.accepted_without_message_id, report.failed
                    );
                    self.error = report.failed > 0;
                    self.report = Some(report);
                    self.receiver = None;
                    self.sending = false;
                }
                Event::Error(error) => {
                    if !self.sending {
                        self.preview_source = None;
                    }
                    self.status = error;
                    self.error = true;
                    self.receiver = None;
                    self.sending = false;
                }
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("邮件工作台").size(20.0).strong().color(INK));
            ui.label(
                RichText::new(if self.pending_since.is_some() {
                    "正在保存…"
                } else {
                    "自动保存"
                })
                .size(12.0)
                .color(ACCENT),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(self.receiver.is_none(), primary("发送邮件"))
                    .clicked()
                {
                    self.start(ui.ctx(), true);
                }
                if ui
                    .add_enabled(
                        self.receiver.is_none(),
                        egui::Button::new("生成预览").min_size(Vec2::new(96.0, 34.0)),
                    )
                    .clicked()
                {
                    self.start(ui.ctx(), false);
                }
                ui.add_enabled_ui(self.receiver.is_none(), |ui| {
                    if ui.button("导出配置").clicked() {
                        self.save_config();
                    }
                    if ui.button("新建邮件").clicked() {
                        self.new_draft();
                    }
                    if ui.button("打开配置").clicked() {
                        self.open_config();
                    }
                });
            });
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.visuals_mut().override_text_color = Some(Color32::from_rgb(218, 230, 237));
        ui.add_space(12.0);
        let logo = self
            .logo
            .get_or_insert_with(|| branding::logo_texture(ui.ctx()));
        egui::Frame::new()
            .fill(Color32::WHITE)
            .corner_radius(10)
            .inner_margin(8)
            .show(ui, |ui| {
                let width = ui.available_width();
                let height = width * logo.size_vec2().y / logo.size_vec2().x;
                ui.add(
                    egui::Image::new((logo.id(), logo.size_vec2()))
                        .fit_to_exact_size(Vec2::new(width, height))
                        .alt_text("ShrimpAPI Logo"),
                );
            });
        ui.add_space(8.0);
        ui.label(
            RichText::new("EMAIL STUDIO")
                .size(11.0)
                .color(Color32::from_rgb(124, 160, 173)),
        );
        ui.add_space(36.0);
        for (number, page) in [
            ("01", Page::Compose),
            ("02", Page::Templates),
            ("03", Page::Recipients),
            ("04", Page::Variables),
            ("05", Page::Connection),
            ("06", Page::Results),
        ] {
            let text = format!("{number}   {}", page.title());
            let selected = self.page == page;
            if ui
                .add(
                    egui::Button::new(RichText::new(text).size(15.0).color(Color32::WHITE))
                        .min_size(Vec2::new(ui.available_width(), 44.0))
                        .corner_radius(8)
                        .fill(if selected {
                            ACCENT
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(Stroke::NONE),
                )
                .clicked()
            {
                self.page = page;
            }
            ui.add_space(6.0);
        }
        ui.add_space(28.0);
        ui.separator();
        ui.add_space(18.0);
        ui.label(
            RichText::new(format!("{} 位收件人", self.draft.recipients.len()))
                .size(17.0)
                .strong(),
        );
        ui.label(format!("{} 条匹配规则", self.draft.rules.len()));
        ui.add_space(18.0);
        ui.label(
            RichText::new("Cloudflare SMTP")
                .size(12.0)
                .color(Color32::from_rgb(124, 160, 173)),
        );
        ui.label(RichText::new("隐式 TLS · 465").size(12.0));
        ui.add_space(24.0);
        if ui
            .add_enabled(self.receiver.is_none(), egui::Button::new("载入示例"))
            .clicked()
        {
            match CampaignEditor::example() {
                Ok(draft) => {
                    self.draft = draft;
                    self.config_path = None;
                    self.status = "示例已载入，可生成预览。".into();
                    self.error = false;
                }
                Err(error) => self.notify(Err(error), ""),
            }
        }
        if let Some(path) = &self.config_path {
            ui.add_space(20.0);
            ui.label(
                RichText::new(path.display().to_string())
                    .size(11.0)
                    .color(Color32::from_rgb(124, 160, 173)),
            );
        }
    }

    fn templates(&mut self, ui: &mut egui::Ui) {
        page_heading(
            ui,
            "模板管理",
            "保存多个常用模板；应用时替换主题、正文、全局变量和规则，保留发件身份及收件人。",
        );
        card().show(ui, |ui| {
            ui.strong("模板库");
            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button("从当前邮件新建模板").clicked() {
                    let result = self
                        .library
                        .add("新模板", TemplateContent::capture(&self.draft));
                    match result {
                        Ok(id) => self.selected_template = Some(id),
                        Err(error) => self.notify(Err(error), ""),
                    }
                }
                if ui.button("新建空白模板").clicked() {
                    match self.library.add(
                        "空白模板",
                        TemplateContent {
                            html_enabled: true,
                            ..Default::default()
                        },
                    ) {
                        Ok(id) => self.selected_template = Some(id),
                        Err(error) => self.notify(Err(error), ""),
                    }
                }
                if ui.button("导入配置为模板").clicked()
                    && let Some(path) = json_dialog().pick_file()
                {
                    let result = CampaignEditor::load(&path).and_then(|draft| {
                        self.library.add(
                            path.file_stem()
                                .and_then(|stem| stem.to_str())
                                .unwrap_or("导入模板"),
                            TemplateContent::capture(&draft),
                        )
                    });
                    match result {
                        Ok(id) => self.selected_template = Some(id),
                        Err(error) => self.notify(Err(error), ""),
                    }
                }
            });
            ui.add_space(14.0);
            let mut startup = self.library.startup_template;
            for template in &mut self.library.templates {
                ui.push_id(("template", template.id), |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(
                                self.selected_template == Some(template.id),
                                &template.name,
                            )
                            .clicked()
                        {
                            self.selected_template = Some(template.id);
                        }
                        ui.checkbox(&mut template.favorite, "常用");
                        if ui.radio(startup == Some(template.id), "启动默认").clicked() {
                            startup = Some(template.id);
                        }
                    });
                });
            }
            self.library.startup_template = startup;
            if ui
                .radio(
                    self.library.startup_template.is_none(),
                    "新建邮件使用空白内容",
                )
                .clicked()
            {
                self.library.startup_template = None;
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "可将多个模板设为常用。新建邮件使用启动默认模板；重新打开应用会恢复上次草稿。",
                )
                .small()
                .color(MUTED),
            );
        });
        ui.add_space(18.0);
        let Some(id) = self.selected_template else {
            ui.label("先新建或选择一个模板。");
            return;
        };
        let Some(index) = self
            .library
            .templates
            .iter()
            .position(|template| template.id == id)
        else {
            return;
        };
        let mut apply = false;
        let mut duplicate = false;
        let mut remove = false;
        let mut replace = false;
        let mut export = false;
        card().show(ui, |ui| {
            let template = &mut self.library.templates[index];
            ui.label(RichText::new("模板名称").size(13.0).color(MUTED));
            let name_response = ui.add(
                egui::TextEdit::singleline(&mut template.name)
                    .hint_text("例如：产品更新通知")
                    .desired_width(f32::INFINITY),
            );
            if name_response.lost_focus() && template.name.trim().is_empty() {
                template.name = "未命名模板".into();
            }
            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                apply = ui.add(primary("应用到邮件")).clicked();
                duplicate = ui.button("复制模板").clicked();
                replace = ui.button("用当前邮件更新").clicked();
                export = ui.button("导出模板").clicked();
                remove = ui.button("删除模板").clicked();
            });
            ui.add_space(18.0);
            field(
                ui,
                "主题模板",
                &mut template.content.subject,
                "{{name}}，你好",
            );
            ui.add_space(12.0);
            ui.checkbox(&mut template.content.html_enabled, "启用 HTML 正文");
            if template.content.html_enabled {
                egui::ScrollArea::vertical()
                    .id_salt("template-html")
                    .max_height(240.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut template.content.html)
                                .code_editor()
                                .desired_width(f32::INFINITY)
                                .desired_rows(10),
                        );
                    });
            }
            ui.add_space(12.0);
            ui.checkbox(&mut template.content.text_enabled, "启用纯文本正文");
            if template.content.text_enabled {
                ui.add(
                    egui::TextEdit::multiline(&mut template.content.text)
                        .desired_width(f32::INFINITY)
                        .desired_rows(6),
                );
            }
            egui::CollapsingHeader::new("模板默认变量")
                .id_salt("template-variables")
                .show(ui, |ui| variable_table(ui, &mut template.content.variables));
            egui::CollapsingHeader::new("模板匹配规则")
                .id_salt("template-rules")
                .show(ui, |ui| {
                    if ui.button("添加规则").clicked() {
                        template.content.rules.push(RuleRow::default());
                    }
                    let mut deleted = None;
                    let mut movement = None;
                    let count = template.content.rules.len();
                    for (rule_index, rule) in template.content.rules.iter_mut().enumerate() {
                        ui.push_id(rule_index, |ui| {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut rule.pattern)
                                        .hint_text("*@example.com")
                                        .desired_width(260.0),
                                );
                                if ui
                                    .add_enabled(rule_index > 0, egui::Button::new("上移"))
                                    .clicked()
                                {
                                    movement = Some((rule_index, rule_index - 1));
                                }
                                if ui
                                    .add_enabled(rule_index + 1 < count, egui::Button::new("下移"))
                                    .clicked()
                                {
                                    movement = Some((rule_index, rule_index + 1));
                                }
                                if ui.button("移除规则").clicked() {
                                    deleted = Some(rule_index);
                                }
                            });
                            variable_table(ui, &mut rule.variables);
                            ui.separator();
                        });
                    }
                    if let Some(index) = deleted {
                        template.content.rules.remove(index);
                    } else if let Some((a, b)) = movement {
                        template.content.rules.swap(a, b);
                    }
                });
        });
        if replace {
            self.library.templates[index].content = TemplateContent::capture(&self.draft);
        }
        if duplicate {
            let template = self.library.templates[index].clone();
            match self
                .library
                .add(&format!("{} 副本", template.name), template.content)
            {
                Ok(id) => self.selected_template = Some(id),
                Err(error) => self.notify(Err(error), ""),
            }
        }
        if export && let Some(path) = json_dialog().set_file_name("template.json").save_file() {
            let result = self.library.templates[index]
                .content
                .editor()
                .save(&path.with_extension("json"));
            self.notify(result, "模板已导出，可在模板管理中重新导入。");
        }
        if remove {
            self.library.remove(id);
            self.selected_template = self.library.templates.first().map(|template| template.id);
        } else if apply {
            self.apply_template(id);
        }
    }

    fn storage_panel(&mut self, ui: &mut egui::Ui) {
        card().show(ui, |ui| {
            ui.strong("设置与数据");
            ui.add_space(10.0);
            ui.label("草稿、收件人、模板库、连接和界面设置会自动保存，更新应用后自动恢复。");
            if let Some(store) = &self.store {
                ui.label(
                    RichText::new(store.directory.display().to_string())
                        .small()
                        .color(MUTED),
                );
            }
            ui.label(
                RichText::new(&self.storage_status).color(if self.storage_error {
                    Color32::from_rgb(190, 65, 65)
                } else {
                    ACCENT
                }),
            );
            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button("立即保存全部设置").clicked() {
                    self.persist_now();
                }
                if ui.button("打开数据目录").clicked()
                    && let Some(store) = &self.store
                {
                    let result = fs::create_dir_all(&store.directory)
                        .context("无法创建数据目录")
                        .and_then(|_| open::that(&store.directory).context("无法打开数据目录"));
                    self.notify(result, "已打开应用数据目录。");
                }
                if ui.button("备份设置").clicked()
                    && let Some(path) = json_dialog()
                        .set_file_name("shrimpapi-settings.json")
                        .save_file()
                {
                    let result = self.snapshot().export(&path.with_extension("json"));
                    self.notify(
                        result,
                        "设置和模板库已备份；API 令牌保存在本机系统凭据库中。",
                    );
                }
                if ui.button("恢复设置备份").clicked()
                    && let Some(path) = json_dialog().pick_file()
                {
                    let result = fs::read(path)
                        .context("无法读取设置备份")
                        .and_then(|bytes| AppSettings::decode(&bytes));
                    match result {
                        Ok(settings) => {
                            let mut restored = Self::restore(settings);
                            restored.store = self.store.clone();
                            restored.token.clone_from(&self.token);
                            restored.saved_token.clone_from(&self.saved_token);
                            restored.persisted = None;
                            restored.browser_files = std::mem::take(&mut self.browser_files);
                            *self = restored;
                            self.persist_now();
                            self.notify(
                                Ok(()),
                                "已恢复模板库和全部设置，API 令牌继续使用本机凭据。",
                            );
                        }
                        Err(error) => self.notify(Err(error), ""),
                    }
                }
            });
            ui.add_space(8.0);
            ui.label(
                RichText::new("备份用于手动迁移到另一台电脑；API 令牌需在新电脑重新填写。")
                    .small()
                    .color(MUTED),
            );
        });
    }

    fn compose(&mut self, ui: &mut egui::Ui) {
        page_heading(ui, "邮件内容", "编辑主题和正文，为每个收件人生成专属邮件。");
        ui.horizontal_wrapped(|ui| {
            ui.label("常用模板");
            let templates: Vec<_> = self
                .library
                .templates
                .iter()
                .filter(|template| template.favorite)
                .map(|template| (template.id, template.name.clone()))
                .collect();
            for (id, name) in templates {
                if ui.button(name).clicked() {
                    self.apply_template(id);
                }
            }
            if ui.button("管理模板").clicked() {
                self.page = Page::Templates;
            }
        });
        ui.add_space(12.0);
        ui.columns(2, |columns| {
            card().show(&mut columns[0], |ui| {
                ui.set_min_width(ui.available_width());
                ui.strong("邮件模板");
                ui.add_space(16.0);
                field(ui, "主题", &mut self.draft.subject, "你好，{{name}}");
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.edit_html, true, "HTML 正文");
                    ui.selectable_value(&mut self.edit_html, false, "纯文本正文");
                });
                ui.add_space(8.0);
                let (enabled, body) = if self.edit_html {
                    (&mut self.draft.html_enabled, &mut self.draft.html)
                } else {
                    (&mut self.draft.text_enabled, &mut self.draft.text)
                };
                ui.horizontal(|ui| {
                    ui.checkbox(enabled, "启用此正文");
                    if ui.button("从文件导入").clicked()
                        && let Some(path) = rfd::FileDialog::new().pick_file()
                    {
                        match fs::read_to_string(path) {
                            Ok(content) => {
                                *body = content;
                                *enabled = true;
                            }
                            Err(error) => {
                                self.status = error.to_string();
                                self.error = true;
                            }
                        }
                    }
                });
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .id_salt("body-editor")
                    .max_height(430.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(body)
                                .code_editor()
                                .desired_width(f32::INFINITY)
                                .desired_rows(24),
                        );
                    });
                ui.add_space(14.0);
                ui.label(RichText::new("内置占位符").small().color(MUTED));
                ui.horizontal_wrapped(|ui| {
                    for variable in ["{{name}}", "{{email}}", "{{local}}", "{{domain}}"] {
                        if ui.small_button(variable).clicked() {
                            body.push_str(variable);
                        }
                    }
                });
            });
            card().show(&mut columns[1], |ui| self.preview_panel(ui));
        });
    }

    fn preview_panel(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.strong("收件人预览");
            ui.label(
                RichText::new(format!("{} 封", self.preview.len()))
                    .small()
                    .color(ACCENT),
            );
        });
        ui.add_space(16.0);
        if self.preview_source.as_ref() != Some(&self.draft) || self.preview.is_empty() {
            ui.add_space(32.0);
            ui.label(RichText::new("生成一份最新预览").size(20.0).color(INK));
            ui.add_space(8.0);
            ui.label("点击上方「生成预览」，检查每位收件人的主题、正文和变量替换结果。");
            ui.label(
                RichText::new("预览不需要 API 令牌，也不会发送邮件。")
                    .small()
                    .color(MUTED),
            );
            return;
        }
        egui::ComboBox::from_id_salt("preview-recipient")
            .selected_text(&self.preview[self.selected_recipient].recipient)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for (index, email) in self.preview.iter().enumerate() {
                    ui.selectable_value(&mut self.selected_recipient, index, &email.recipient);
                }
            });
        ui.add_space(18.0);
        let email = &self.preview[self.selected_recipient];
        ui.label(RichText::new(&email.subject).size(20.0).strong().color(INK));
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.preview_html, false, "纯文本");
            ui.selectable_value(&mut self.preview_html, true, "HTML 源码");
        });
        ui.add_space(10.0);
        let mut body = if self.preview_html {
            email.html.as_deref()
        } else {
            email.text.as_deref()
        }
        .unwrap_or("未提供此格式的正文，可切换格式查看。");
        egui::ScrollArea::vertical()
            .id_salt("rendered-body")
            .max_height(350.0)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut body)
                        .desired_width(f32::INFINITY)
                        .desired_rows(18)
                        .font(if self.preview_html {
                            egui::TextStyle::Monospace
                        } else {
                            egui::TextStyle::Body
                        }),
                );
            });
        ui.add_space(16.0);
        let has_html = email.html.is_some();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(has_html, egui::Button::new("浏览器预览 HTML"))
                .clicked()
            {
                let result = self.open_html();
                self.notify(result, "已在默认浏览器打开当前收件人的 HTML 邮件。");
            }
            if ui.button("导出全部预览").clicked()
                && let Some(path) = rfd::FileDialog::new().pick_folder()
            {
                let result = export_previews(&path, &self.preview);
                self.notify(result, "已导出 HTML、纯文本、EML 和预览清单。");
            }
        });
    }

    fn recipients(&mut self, ui: &mut egui::Ui) {
        page_heading(ui, "收件人", "添加目标邮箱，并为每个人设置称呼和专属变量。");
        card().show(ui, |ui| {
            ui.strong("批量添加");
            ui.label(
                RichText::new("使用换行、英文逗号或分号分隔邮箱。")
                    .small()
                    .color(MUTED),
            );
            ui.add(
                egui::TextEdit::multiline(&mut self.bulk_addresses)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .hint_text("alice@example.com\nbob@example.net"),
            );
            ui.horizontal(|ui| {
                if ui.button("添加到列表").clicked() {
                    self.draft.add_addresses(&self.bulk_addresses);
                    self.bulk_addresses.clear();
                }
                if ui.button("导入收件人 JSON").clicked()
                    && let Some(path) = json_dialog().pick_file()
                {
                    match load_recipients(&path) {
                        Ok(recipients) => self
                            .draft
                            .recipients
                            .extend(recipients.into_iter().map(RecipientRow::from_recipient)),
                        Err(error) => self.notify(Err(error), ""),
                    }
                }
                if ui.button("新增一行").clicked() {
                    self.draft.recipients.push(RecipientRow::default());
                }
            });
        });
        ui.add_space(16.0);
        let mut remove = None;
        for (index, recipient) in self.draft.recipients.iter_mut().enumerate() {
            ui.push_id(("recipient", index), |ui| {
                card().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{:02}", index + 1)).color(MUTED));
                        ui.add(
                            egui::TextEdit::singleline(&mut recipient.email)
                                .hint_text("邮箱地址")
                                .desired_width(280.0),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut recipient.name)
                                .hint_text("称呼（可选）")
                                .desired_width(160.0),
                        );
                        if ui.button("删除").clicked() {
                            remove = Some(index);
                        }
                    });
                    egui::CollapsingHeader::new(format!(
                        "专属变量 · {}",
                        recipient.variables.len()
                    ))
                    .show(ui, |ui| variable_table(ui, &mut recipient.variables));
                })
            });
            ui.add_space(10.0);
        }
        if let Some(index) = remove {
            self.draft.recipients.remove(index);
        }
        if self.draft.recipients.is_empty() {
            ui.label("收件人列表为空，先添加一个邮箱。");
        }
    }

    fn variables(&mut self, ui: &mut egui::Ui) {
        page_heading(
            ui,
            "变量与匹配规则",
            "全局变量 → 按顺序命中的规则 → 收件人变量，后者覆盖同名值。",
        );
        card().show(ui, |ui| {
            ui.strong("全局变量");
            ui.add_space(12.0);
            variable_table(ui, &mut self.draft.variables);
        });
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            ui.strong("邮箱匹配规则");
            if ui.button("添加规则").clicked() {
                self.draft.rules.push(RuleRow::default());
            }
        });
        ui.label(RichText::new("例如 *@example.com、alice@example.com 或 sales+*@example.com。按完整邮箱匹配，忽略大小写。").small().color(MUTED));
        ui.add_space(12.0);
        let mut remove = None;
        let mut movement = None;
        let count = self.draft.rules.len();
        for (index, rule) in self.draft.rules.iter_mut().enumerate() {
            ui.push_id(("rule", index), |ui| {
                card().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(format!("规则 {}", index + 1));
                        ui.add(
                            egui::TextEdit::singleline(&mut rule.pattern)
                                .hint_text("*@example.com")
                                .desired_width(290.0),
                        );
                        if ui
                            .add_enabled(index > 0, egui::Button::new("上移"))
                            .clicked()
                        {
                            movement = Some((index, index - 1));
                        }
                        if ui
                            .add_enabled(index + 1 < count, egui::Button::new("下移"))
                            .clicked()
                        {
                            movement = Some((index, index + 1));
                        }
                        if ui.button("删除").clicked() {
                            remove = Some(index);
                        }
                    });
                    ui.add_space(12.0);
                    variable_table(ui, &mut rule.variables);
                })
            });
            ui.add_space(12.0);
        }
        if let Some(index) = remove {
            self.draft.rules.remove(index);
        } else if let Some((a, b)) = movement {
            self.draft.rules.swap(a, b);
        }
    }

    fn connection(&mut self, ui: &mut egui::Ui) {
        page_heading(ui, "连接设置", "配置发件身份与 Cloudflare API 令牌。");
        card().show(ui, |ui| {
            ui.set_max_width(760.0);
            ui.strong("发件身份");
            ui.add_space(16.0);
            field(
                ui,
                "发件人",
                &mut self.draft.from,
                "Name <hello@yourdomain.com>",
            );
            ui.add_space(12.0);
            field(
                ui,
                "回复邮箱（可选）",
                &mut self.draft.reply_to,
                "support@yourdomain.com",
            );
            ui.add_space(24.0);
            ui.separator();
            ui.add_space(20.0);
            ui.strong("Cloudflare SMTP");
            ui.add_space(12.0);
            egui::Grid::new("smtp-settings")
                .min_col_width(190.0)
                .spacing([30.0, 12.0])
                .show(ui, |ui| {
                    ui.label("服务器");
                    ui.monospace(SMTP_HOST);
                    ui.end_row();
                    ui.label("端口 / 安全连接");
                    ui.label(format!("{SMTP_PORT} / 隐式 TLS"));
                    ui.end_row();
                    ui.label("认证用户名");
                    ui.monospace("api_token");
                    ui.end_row();
                    ui.label("超时（秒）");
                    ui.add(egui::DragValue::new(&mut self.timeout_secs).range(1..=300));
                    ui.end_row();
                });
            ui.add_space(22.0);
            let label = ui.label("API 令牌");
            ui.add(
                egui::TextEdit::singleline(&mut self.token)
                    .password(!self.show_token)
                    .desired_width(f32::INFINITY)
                    .hint_text("Cloudflare API token"),
            )
            .labelled_by(label.id);
            ui.checkbox(&mut self.show_token, "显示令牌");
            ui.add_space(10.0);
            ui.label(
                RichText::new("令牌自动保存到系统凭据库，下次启动自动读取；不会写入 JSON 配置或备份。清空令牌后会移除已保存的凭据。")
                    .small()
                    .color(MUTED),
            );
            ui.label(
                RichText::new(
                    "发件域名需要已接入 Email Sending，令牌需要 Email Sending: Edit 权限。",
                )
                .small()
                .color(MUTED),
            );
        });
        ui.add_space(20.0);
        self.storage_panel(ui);
    }

    fn results(&mut self, ui: &mut egui::Ui) {
        page_heading(
            ui,
            "发送记录",
            "逐封查看 SMTP 提交结果与服务端 Message-ID。",
        );
        card().show(ui, |ui| {
            if self.sending {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("正在发送，已处理 {} 个收件人", self.results.len()));
                });
                ui.add(
                    egui::ProgressBar::new(
                        self.results.len() as f32 / self.draft.recipients.len().max(1) as f32,
                    )
                    .animate(true),
                );
            } else if let Some(report) = &self.report {
                ui.horizontal(|ui| {
                    metric(ui, "已接受", report.accepted, ACCENT);
                    metric(
                        ui,
                        "无 Message-ID",
                        report.accepted_without_message_id,
                        MUTED,
                    );
                    metric(ui, "失败", report.failed, Color32::from_rgb(190, 65, 65));
                });
                ui.add_space(14.0);
                if ui.button("导出 JSON 报告").clicked()
                    && let Some(path) = json_dialog().set_file_name("send-report.json").save_file()
                {
                    let result = serde_json::to_vec_pretty(report)
                        .context("无法生成报告")
                        .and_then(|bytes| fs::write(path, bytes).context("无法保存报告"));
                    self.notify(result, "发送报告已保存。");
                }
            } else {
                ui.label("尚无发送记录。完成配置后，点击右上角「发送邮件」。");
            }
            ui.add_space(12.0);
            ui.label(
                RichText::new(
                    "SMTP 接受后的最终投递结果，可在 Cloudflare Email Sending 日志中确认。",
                )
                .small()
                .color(MUTED),
            );
        });
        ui.add_space(16.0);
        for result in &self.results {
            card().show(ui, |ui| {
                let (label, color) = match result.status {
                    SendStatus::Accepted => ("已接受", ACCENT),
                    SendStatus::AcceptedWithoutMessageId => ("已接受 · 无 Message-ID", MUTED),
                    SendStatus::Failed => ("提交失败", Color32::from_rgb(190, 65, 65)),
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).color(color).strong());
                    ui.strong(&result.recipient);
                });
                if let Some(id) = &result.message_id {
                    ui.monospace(id);
                }
                if let Some(response) = &result.response {
                    ui.label(response);
                }
                if let Some(error) = &result.error {
                    ui.label(RichText::new(error).color(color));
                }
            });
            ui.add_space(10.0);
        }
    }

    fn open_config(&mut self) {
        if let Some(path) = json_dialog().pick_file() {
            match CampaignEditor::load(&path) {
                Ok(draft) => {
                    self.draft = draft;
                    self.config_path = Some(path);
                    self.preview.clear();
                    self.preview_source = None;
                    self.selected_recipient = 0;
                    self.notify(Ok(()), "配置和模板已载入。");
                }
                Err(error) => self.notify(Err(error), ""),
            }
        }
    }

    fn save_config(&mut self) {
        let mut dialog = json_dialog();
        dialog = if let Some(path) = &self.config_path {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            dialog.set_file_name(name)
        } else {
            dialog.set_file_name("campaign.json")
        };
        if let Some(path) = dialog.save_file() {
            let path = path.with_extension("json");
            let result = self.draft.save(&path);
            if result.is_ok() {
                self.config_path = Some(path);
            }
            self.notify(
                result,
                "配置与正文模板已保存，下次可通过「打开配置」继续编辑。",
            );
        }
    }

    fn open_html(&mut self) -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("email.html");
        fs::write(
            &path,
            self.preview[self.selected_recipient]
                .html
                .as_deref()
                .context("此邮件没有 HTML 正文")?,
        )?;
        open::that(&path).context("无法打开默认浏览器")?;
        self.browser_files.push(directory);
        Ok(())
    }
}

impl EmailApp {
    fn draw(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("navigation")
            .exact_size(210.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(INK).inner_margin(22))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("sidebar-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.sidebar(ui));
            });
        egui::Panel::top("toolbar")
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(20))
            .show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(12))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if self.receiver.is_some() {
                        ui.spinner();
                    }
                    if self.storage_error {
                        ui.colored_label(Color32::from_rgb(190, 65, 65), &self.storage_status);
                    }
                    ui.label(RichText::new(&self.status).size(12.0).color(if self.error {
                        Color32::from_rgb(190, 65, 65)
                    } else {
                        MUTED
                    }));
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BACKGROUND).inner_margin(26))
            .show(ui, |ui| {
                // A running job owns an immutable snapshot; lock editors until it completes.
                let editable = self.receiver.is_none() || self.page == Page::Results;
                ui.add_enabled_ui(editable, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("page-content")
                        .show(ui, |ui| match self.page {
                            Page::Compose => self.compose(ui),
                            Page::Templates => self.templates(ui),
                            Page::Recipients => self.recipients(ui),
                            Page::Variables => self.variables(ui),
                            Page::Connection => self.connection(ui),
                            Page::Results => self.results(ui),
                        });
                });
            });
        self.autosave(ui.ctx());
        if ui.ctx().input(|input| input.viewport().close_requested()) {
            if self.sending {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.status = "正在提交邮件，请等待当前批次完成后关闭窗口。".into();
            } else {
                self.persist_now();
                if self.storage_error {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    self.page = Page::Connection;
                }
            }
        }
    }
}

impl eframe::App for EmailApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist_now();
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.persist_now();
    }

    fn auto_save_interval(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

fn configure_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Light);
    ctx.global_style_mut(|style| {
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Monospace, egui::FontId::monospace(13.0));
        style.spacing.item_spacing = Vec2::new(10.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 7.0);
        style.visuals.selection.bg_fill = ACCENT;
        style.visuals.widgets.active.bg_fill = ACCENT;
        style.visuals.widgets.inactive.corner_radius = 6.into();
        style.visuals.widgets.hovered.corner_radius = 6.into();
        style.visuals.widgets.active.corner_radius = 6.into();
        style.visuals.extreme_bg_color = Color32::from_rgb(249, 250, 252);
    });
}

fn primary(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).color(Color32::WHITE).strong())
        .fill(ACCENT)
        .stroke(Stroke::NONE)
        .corner_radius(7)
        .min_size(Vec2::new(108.0, 36.0))
}

fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(12)
        .inner_margin(20)
}

fn page_heading(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.label(RichText::new(title).size(26.0).strong().color(INK));
    ui.add_space(3.0);
    ui.label(RichText::new(description).color(MUTED));
    ui.add_space(22.0);
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) {
    let label = ui.label(RichText::new(label).size(13.0).color(MUTED));
    ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(hint)
            .desired_width(f32::INFINITY),
    )
    .labelled_by(label.id);
}

fn metric(ui: &mut egui::Ui, label: &str, value: usize, color: Color32) {
    ui.vertical(|ui| {
        ui.set_min_width(150.0);
        ui.label(
            RichText::new(value.to_string())
                .size(30.0)
                .strong()
                .color(color),
        );
        ui.label(RichText::new(label).color(MUTED));
    });
}

fn variable_table(ui: &mut egui::Ui, rows: &mut Vec<VariableRow>) {
    let mut remove = None;
    egui::Grid::new("variable-grid")
        .num_columns(4)
        .spacing([10.0, 10.0])
        .show(ui, |ui| {
            ui.label(RichText::new("变量名").small().color(MUTED));
            ui.label(RichText::new("替换值").small().color(MUTED));
            ui.label(RichText::new("类型").small().color(MUTED));
            ui.label("");
            ui.end_row();
            for (index, row) in rows.iter_mut().enumerate() {
                ui.add(
                    egui::TextEdit::singleline(&mut row.key)
                        .hint_text("product")
                        .min_size(Vec2::new(150.0, 0.0))
                        .desired_width(150.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut row.value)
                        .hint_text("ShrimpAPI")
                        .min_size(Vec2::new(280.0, 0.0))
                        .desired_width(280.0),
                );
                egui::ComboBox::from_id_salt(("type", index))
                    .width(70.0)
                    .selected_text(if row.json { "JSON" } else { "文本" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut row.json, false, "文本");
                        ui.selectable_value(&mut row.json, true, "JSON");
                    });
                if ui.small_button("删除").clicked() {
                    remove = Some(index);
                }
                ui.end_row();
            }
        });
    if let Some(index) = remove {
        rows.remove(index);
    }
    if ui.button("添加变量").clicked() {
        rows.push(VariableRow::default());
    }
}

fn json_dialog() -> rfd::FileDialog {
    rfd::FileDialog::new().add_filter("JSON", &["json"])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_shapes(shape: &egui::epaint::Shape, output: &mut Vec<(String, egui::Pos2)>) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                output.push((text.galley.job.text.clone(), text.pos))
            }
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    text_shapes(shape, output);
                }
            }
            _ => {}
        }
    }

    fn render(ctx: &egui::Context, draw: impl FnMut(&mut egui::Ui)) -> Vec<(String, egui::Pos2)> {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1360.0, 880.0),
                )),
                ..Default::default()
            },
            draw,
        );
        // These tests inspect layout without a GPU texture consumer.
        output.textures_delta.clear();
        let mut texts = Vec::new();
        for clipped in &output.shapes {
            text_shapes(&clipped.shape, &mut texts);
        }
        texts
    }

    #[test]
    fn all_pages_render_headlessly_with_their_controls() {
        let ctx = egui::Context::default();
        configure_style(&ctx);
        let mut app = EmailApp::new(CampaignEditor::example().unwrap());
        for (page, expected) in [
            (Page::Compose, "邮件模板"),
            (Page::Templates, "模板库"),
            (Page::Recipients, "批量添加"),
            (Page::Variables, "全局变量"),
            (Page::Connection, "认证用户名"),
            (Page::Results, "尚无发送记录"),
        ] {
            app.page = page;
            let texts = render(&ctx, |ui| app.draw(ui));
            assert!(
                texts.iter().any(|(text, _)| text.contains(expected)),
                "missing {expected}"
            );
            assert!(!texts.iter().any(|(text, _)| text.contains("ID clash")));
        }
    }

    #[test]
    fn variable_values_render_in_their_own_grid_column() {
        let ctx = egui::Context::default();
        configure_style(&ctx);
        let mut rows = vec![VariableRow {
            key: "product".into(),
            value: "ShrimpAPI".into(),
            json: false,
        }];
        let texts = render(&ctx, |ui| {
            egui::CentralPanel::default().show(ui, |ui| variable_table(ui, &mut rows));
        });
        let key = texts.iter().find(|(text, _)| text == "product").unwrap().1;
        let value = texts
            .iter()
            .find(|(text, _)| text == "ShrimpAPI")
            .unwrap()
            .1;
        assert!(
            value.x > key.x + 120.0,
            "key={key:?}, value={value:?}, texts={texts:?}"
        );
        assert!(
            (value.y - key.y).abs() < 2.0,
            "key={key:?}, value={value:?}"
        );
    }

    fn finish_preview(app: &mut EmailApp) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while app.receiver.is_some() {
            app.poll();
            assert!(
                std::time::Instant::now() < deadline,
                "preview worker did not complete"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn edited_preview_is_invalidated_and_worker_regenerates_current_content() {
        let ctx = egui::Context::default();
        let mut app = EmailApp::new(CampaignEditor::example().unwrap());
        app.draft.subject = "Changed {{name}}".into();
        let texts = render(&ctx, |ui| app.preview_panel(ui));
        assert!(texts.iter().any(|(text, _)| text == "生成一份最新预览"));
        app.start(&ctx, false);
        finish_preview(&mut app);
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.preview.len(), 3);
        assert_eq!(app.preview[0].subject, "Changed Alice");
        assert_eq!(app.preview_source.as_ref(), Some(&app.draft));
    }

    #[test]
    fn worker_reports_template_errors_and_releases_busy_state() {
        let ctx = egui::Context::default();
        let mut app = EmailApp::new(CampaignEditor::example().unwrap());
        app.draft.subject = "{{missing_variable}}".into();
        app.start(&ctx, false);
        finish_preview(&mut app);
        assert!(app.error);
        assert!(app.status.contains("missing_variable"));
        assert!(app.preview_source.is_none());
        assert!(!app.sending);
    }

    #[test]
    fn missing_token_opens_settings_without_starting_network_work() {
        let mut app = EmailApp::new(CampaignEditor::example().unwrap());
        app.token.clear();
        app.start(&egui::Context::default(), true);
        assert!(app.page == Page::Connection);
        assert!(app.error);
        assert!(app.receiver.is_none());
        assert!(!app.sending);
    }

    #[derive(Default)]
    struct FakeCredentials {
        token: std::cell::RefCell<String>,
        fail: bool,
    }

    impl CredentialStore for FakeCredentials {
        fn load(&self) -> Result<String> {
            Ok(self.token.borrow().clone())
        }
        fn save(&self, token: &str) -> Result<()> {
            if self.fail {
                anyhow::bail!("credential store unavailable");
            }
            *self.token.borrow_mut() = token.into();
            Ok(())
        }
    }

    #[test]
    fn all_preferences_and_secret_restore_after_restart_using_separate_stores() {
        let directory = tempfile::tempdir().unwrap();
        let credentials = FakeCredentials::default();
        let mut app = EmailApp::new(CampaignEditor::example().unwrap());
        app.store = Some(SettingsStore::new(directory.path().into()));
        app.token = "private-test-api-token".into();
        app.timeout_secs = 90;
        app.page = Page::Templates;
        app.bulk_addresses = "pending@example.net".into();
        app.selected_template = Some(2);
        app.library.templates[1].content.subject = "Saved {{name}}".into();
        app.persist_with(&credentials).unwrap();
        let snapshot = app.snapshot();
        let mut restored = EmailApp::restore(app.store.as_ref().unwrap().load().unwrap().settings);
        restored.token = credentials.load().unwrap();
        assert_eq!(restored.snapshot(), snapshot);
        assert_eq!(restored.token, "private-test-api-token");
        let saved = fs::read_to_string(app.store.as_ref().unwrap().path()).unwrap();
        assert!(!saved.contains("private-test-api-token"));
        app.token.clear();
        app.persist_with(&credentials).unwrap();
        assert!(credentials.load().unwrap().is_empty());
    }

    #[test]
    fn failed_secret_save_keeps_pending_secret_and_retry_persists_it() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = EmailApp::new(CampaignEditor::example().unwrap());
        app.store = Some(SettingsStore::new(directory.path().into()));
        app.token = "pending-secret".into();
        let failure = FakeCredentials {
            fail: true,
            ..Default::default()
        };
        assert!(app.persist_with(&failure).is_err());
        assert!(app.saved_token.is_empty());
        assert_eq!(app.token, "pending-secret");
        assert!(app.store.as_ref().unwrap().path().exists());
        let credentials = FakeCredentials::default();
        app.persist_with(&credentials).unwrap();
        assert_eq!(credentials.load().unwrap(), "pending-secret");
        assert_eq!(app.saved_token, "pending-secret");
    }

    #[test]
    fn applying_template_invalidates_preview_and_new_draft_uses_startup_default() {
        let mut app = EmailApp::new(CampaignEditor::example().unwrap());
        let recipients = app.draft.recipients.clone();
        let from = app.draft.from.clone();
        app.apply_template(3);
        assert_eq!(app.draft.recipients, recipients);
        assert_eq!(app.draft.from, from);
        assert!(app.preview.is_empty());
        assert!(app.preview_source.is_none());
        app.library.startup_template = Some(2);
        app.new_draft();
        assert_eq!(
            app.draft.subject,
            app.library.get(2).unwrap().content.subject
        );
        assert!(app.draft.recipients.is_empty());
        assert_eq!(app.draft.from, from);
    }
}
