use std::fs;

use serde_json::json;
use shrimpapi_email::{
    editor::CampaignEditor,
    settings::{APP_ID, AppSettings, Page, SCHEMA_VERSION, SettingsStore},
    templates::{TemplateContent, TemplateLibrary},
};

#[test]
fn default_library_contains_three_independently_editable_favorites() {
    let library = TemplateLibrary::default();
    assert_eq!(library.templates.len(), 3);
    assert!(library.templates.iter().all(|template| template.favorite));
    assert_eq!(
        library
            .templates
            .iter()
            .map(|template| template.name.as_str())
            .collect::<Vec<_>>(),
        ["新用户注册", "模型上新", "服务通知"]
    );
    let mut draft = CampaignEditor::example().unwrap();
    for template in &library.templates {
        template.content.apply(&mut draft);
        let messages = draft.prepare().unwrap();
        assert_eq!(messages.len(), 3);
        assert!(messages[0].text.as_ref().unwrap().contains("Alice"));
        assert!(messages[0].html.as_ref().unwrap().contains("Alice"));
    }
}

#[test]
fn template_application_keeps_sender_recipients_and_personal_overrides() {
    let mut draft = CampaignEditor::example().unwrap();
    let from = draft.from.clone();
    let reply_to = draft.reply_to.clone();
    let recipients = draft.recipients.clone();
    let library = TemplateLibrary::default();
    library.templates[0].content.apply(&mut draft);
    assert_eq!(draft.from, from);
    assert_eq!(draft.reply_to, reply_to);
    assert_eq!(draft.recipients, recipients);
    assert!(
        draft.prepare().unwrap()[1]
            .text
            .as_ref()
            .unwrap()
            .contains("欢迎使用专业服务，如需接入帮助请联系我们。")
    );
    let exported = serde_json::to_value(TemplateContent::capture(&draft)).unwrap();
    assert!(exported.get("from").is_none());
    assert!(exported.get("recipients").is_none());
}

#[test]
fn template_ids_stay_unique_across_copy_remove_and_restart() {
    let mut library = TemplateLibrary::default();
    let id = library
        .add("自定义", library.templates[0].content.clone())
        .unwrap();
    library.startup_template = Some(id);
    library.remove(id);
    assert!(library.startup_template.is_none());
    library.remove(2);
    let id = library
        .add("复制", library.templates[0].content.clone())
        .unwrap();
    assert!(library.get(id).is_some());
    library.validate().unwrap();
    let restored: TemplateLibrary =
        serde_json::from_slice(&serde_json::to_vec(&library).unwrap()).unwrap();
    assert_eq!(library, restored);
    assert!(library.add(" ", TemplateContent::default()).is_err());
}

#[test]
fn settings_survive_restart_with_custom_templates_incomplete_draft_and_ui() {
    let directory = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(directory.path().join("application-data"));
    let mut state = store.load().unwrap().settings;
    state.draft.from = "Real Sender <hello@real.example>".into();
    state.draft.subject = "{{unfinished".into();
    state.timeout_secs = 83;
    state.ui.page = Page::Templates;
    state.ui.bulk_addresses = "unsubmitted@example.com".into();
    state.ui.edit_html = false;
    state.library.templates[0].name = "我修改的注册模板".into();
    state.library.templates[0].content.html = "<p>Custom {{name}}</p>".into();
    state.library.templates[0].favorite = false;
    let id = state
        .library
        .add("新通知", TemplateContent::capture(&state.draft))
        .unwrap();
    state.library.startup_template = Some(id);
    state.ui.selected_template = Some(id);
    store.save(&state).unwrap();
    let reopened = SettingsStore::new(store.directory.clone()).load().unwrap();
    assert_eq!(reopened.settings, state);
    assert!(reopened.warning.is_none());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(store.path()).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn migration_adds_new_fields_without_losing_existing_user_draft() {
    let legacy = json!({ "subject": "旧版草稿", "from": "hello@old.example", "text": "旧内容", "text_enabled": true });
    let migrated = AppSettings::decode(&serde_json::to_vec(&legacy).unwrap()).unwrap();
    assert_eq!(migrated.version, SCHEMA_VERSION);
    assert_eq!(migrated.draft.subject, "旧版草稿");
    assert_eq!(migrated.draft.text, "旧内容");
    assert_eq!(migrated.library.templates.len(), 3);
    assert_eq!(migrated.timeout_secs, 30);
    let legacy = json!({ "draft": legacy, "timeout_secs": 0 });
    let migrated = AppSettings::decode(&serde_json::to_vec(&legacy).unwrap()).unwrap();
    assert_eq!(migrated.draft.subject, "旧版草稿");
    assert_eq!(migrated.timeout_secs, 1);
}

#[test]
fn upgrading_v1_replaces_entire_old_catalog_once_and_keeps_the_original_backup() {
    let directory = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(directory.path().into());
    let mut old = AppSettings {
        version: 1,
        timeout_secs: 81,
        ..Default::default()
    };
    for (template, name) in
        old.library
            .templates
            .iter_mut()
            .zip(["欢迎邮件", "通知邮件", "活动推广"])
    {
        template.name = name.into();
        template.content.subject = "旧主题".into();
    }
    old.library
        .add("新用户注册", TemplateContent::default())
        .unwrap();
    old.library.startup_template = Some(3);
    old.ui.selected_template = Some(4);
    old.draft.subject = "用户正在编辑的邮件".into();
    old.draft.from = "hello@real.example".into();
    let bytes = serde_json::to_vec_pretty(&old).unwrap();
    fs::write(store.path(), &bytes).unwrap();
    let loaded = store.load().unwrap();
    assert!(loaded.needs_save);
    assert!(loaded.warning.is_some());
    let mut upgraded = loaded.settings;
    assert_eq!(upgraded.version, 2);
    assert_eq!(upgraded.library, TemplateLibrary::default());
    assert_eq!(upgraded.ui.selected_template, Some(1));
    assert_eq!(upgraded.draft, old.draft);
    assert_eq!(upgraded.timeout_secs, old.timeout_secs);
    let original_backup = directory.path().join("settings.v1.backup.json");
    assert_eq!(fs::read(&original_backup).unwrap(), bytes);
    store.save(&upgraded).unwrap();
    upgraded.library.templates[0].content.subject = "升级后修改的注册主题".into();
    upgraded
        .library
        .add("升级后的自定义模板", TemplateContent::default())
        .unwrap();
    store.save(&upgraded).unwrap();
    let reopened = store.load().unwrap();
    assert!(!reopened.needs_save);
    assert_eq!(reopened.settings, upgraded);
    assert_eq!(fs::read(&original_backup).unwrap(), bytes);
}

#[test]
fn each_new_template_renders_its_required_fields_with_recipient_overrides() {
    let library = TemplateLibrary::default();
    let mut draft = CampaignEditor::example().unwrap();
    for (template, expected_fields) in library.templates.iter().zip([
        vec![
            "注册邮箱",
            "https://yourdomain.com/console",
            "https://yourdomain.com/docs",
        ],
        vec!["provider/model-name", "上下文长度", "价格说明"],
        vec!["开始时间", "预计结束", "影响范围", "建议操作"],
    ]) {
        template.content.apply(&mut draft);
        for message in draft.prepare().unwrap() {
            for body in [&message.html, &message.text] {
                let body = body.as_ref().unwrap();
                assert!(body.contains(&message.recipient));
                for field in &expected_fields {
                    assert!(body.contains(field), "{} missing {field}", template.name);
                }
                assert!(!body.contains("{{"));
            }
        }
    }
}

#[test]
fn removing_templates_in_v2_does_not_reinstall_them_on_next_launch() {
    let mut settings = AppSettings::default();
    settings.library.templates.clear();
    settings.library.startup_template = None;
    settings.ui.selected_template = None;
    let restored = AppSettings::decode(&serde_json::to_vec(&settings).unwrap()).unwrap();
    assert_eq!(restored, settings);
}

#[test]
fn corrupt_settings_recover_previous_valid_backup_and_keep_bad_original() {
    let directory = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(directory.path().into());
    let first = AppSettings::default();
    store.save(&first).unwrap();
    let mut second = first.clone();
    second.draft.subject = "新草稿".into();
    store.save(&second).unwrap();
    fs::write(store.path(), "{corrupted").unwrap();
    let restored = store.load().unwrap();
    assert_eq!(restored.settings, first);
    assert!(restored.warning.is_some());
    assert_eq!(
        fs::read_to_string(directory.path().join("settings.damaged.json")).unwrap(),
        "{corrupted"
    );
    store.save(&restored.settings).unwrap();
    assert_eq!(store.load().unwrap().settings, first);
    assert_eq!(
        AppSettings::decode(&fs::read(store.backup_path()).unwrap()).unwrap(),
        first
    );
}

#[test]
fn unrecoverable_or_newer_settings_are_never_silently_replaced() {
    let directory = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(directory.path().into());
    fs::write(store.path(), "bad data").unwrap();
    assert!(store.load().is_err());
    assert_eq!(fs::read_to_string(store.path()).unwrap(), "bad data");
    let future = json!({ "version": SCHEMA_VERSION + 1, "unknown_feature": true });
    fs::write(store.path(), future.to_string()).unwrap();
    AppSettings::default().export(&store.backup_path()).unwrap();
    assert!(store.load().is_err());
    assert!(store.save(&AppSettings::default()).is_err());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(store.path()).unwrap()).unwrap(),
        future
    );
}

#[test]
fn backups_restore_all_settings_without_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("backup.json");
    let settings = AppSettings::default();
    settings.export(&path).unwrap();
    let bytes = fs::read(path).unwrap();
    assert_eq!(AppSettings::decode(&bytes).unwrap(), settings);
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains("api_token"));
    assert!(!text.contains("password"));
    assert!(!text.contains("credential"));
    assert!(AppSettings::decode(b"{}").is_err());
    assert!(AppSettings::decode(b"[]").is_err());
}

#[test]
fn storage_identity_is_stable_across_app_versions_and_install_locations() {
    let store = SettingsStore::system().unwrap();
    assert_eq!(APP_ID, "com.shrimpapi.email");
    assert!(!store.directory.starts_with(env!("CARGO_MANIFEST_DIR")));
    assert!(
        !store
            .directory
            .to_string_lossy()
            .contains(env!("CARGO_PKG_VERSION"))
    );
    #[cfg(target_os = "macos")]
    assert!(
        store
            .directory
            .ends_with("Library/Application Support/com.shrimpapi.email")
    );
}
