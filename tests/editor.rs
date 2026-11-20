use std::{fs, path::Path};

use serde_json::json;
use shrimpapi_email::{
    Campaign,
    editor::{CampaignEditor, RecipientRow, RuleRow, VariableRow},
    prepare,
};

#[test]
fn visual_editor_renders_without_template_files() {
    let draft = CampaignEditor::example().unwrap();
    let messages = draft.prepare().unwrap();
    assert_eq!(messages.len(), 3);
    assert!(
        messages[0]
            .html
            .as_ref()
            .unwrap()
            .contains("https://yourdomain.com/console/alice")
    );
    assert!(
        messages[1]
            .text
            .as_ref()
            .unwrap()
            .contains("https://yourdomain.com/console/bob")
    );
    assert_eq!(draft.campaign().unwrap().html, None);
}

#[test]
fn saving_editor_and_reopening_preserves_types_templates_and_precedence() {
    let mut draft = CampaignEditor::example().unwrap();
    draft.variables.push(VariableRow {
        key: "preview_flag".into(),
        value: "false".into(),
        json: true,
    });
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("nested/campaign.json");
    draft.save(&path).unwrap();
    let reopened = CampaignEditor::load(&path).unwrap();
    assert_eq!(draft.html, reopened.html);
    assert_eq!(draft.text, reopened.text);
    assert_eq!(
        serde_json::to_value(draft.campaign().unwrap()).unwrap(),
        serde_json::to_value(reopened.campaign().unwrap()).unwrap(),
    );
    let campaign = Campaign::load(&path).unwrap();
    assert_eq!(campaign.variables["preview_flag"], false);
    assert!(campaign.html.unwrap().is_file());
    let messages = prepare(&Campaign::load(&path).unwrap()).unwrap();
    assert_eq!(messages[0].subject, "Alice，欢迎注册 ShrimpAPI");
    let stored: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(stored["html"], "campaign.html");
    assert_eq!(stored["text"], "campaign.txt");
    assert!(stored.get("token").is_none());
}

#[test]
fn graphical_variable_types_keep_false_distinct_from_string_false() {
    let mut draft = CampaignEditor {
        from: "hello@sender.example".into(),
        subject: "Hello".into(),
        text_enabled: true,
        text: "{{#if enabled}}yes{{else}}no{{/if}} {{account.id}}".into(),
        recipients: vec![RecipientRow {
            email: "alice@example.com".into(),
            ..Default::default()
        }],
        variables: vec![
            VariableRow {
                key: "enabled".into(),
                value: "false".into(),
                json: true,
            },
            VariableRow {
                key: "account".into(),
                value: json!({"id": 42}).to_string(),
                json: true,
            },
        ],
        ..Default::default()
    };
    assert_eq!(draft.prepare().unwrap()[0].text.as_deref(), Some("no 42"));
    draft.variables[0].json = false;
    assert_eq!(draft.prepare().unwrap()[0].text.as_deref(), Some("yes 42"));
}

#[test]
fn editor_rejects_duplicates_invalid_json_and_reserved_identity_variables() {
    let mut draft = CampaignEditor::example().unwrap();
    draft.variables.push(draft.variables[0].clone());
    assert!(
        draft
            .campaign()
            .unwrap_err()
            .to_string()
            .contains("重复变量")
    );
    draft.variables.pop();
    draft.variables.push(VariableRow {
        key: "flag".into(),
        value: "[invalid".into(),
        json: true,
    });
    assert!(draft.campaign().unwrap_err().to_string().contains("JSON"));
    draft.variables.pop();
    draft.rules.push(RuleRow {
        pattern: "*".into(),
        variables: vec![VariableRow {
            key: "email".into(),
            value: "someone@example.com".into(),
            json: false,
        }],
    });
    assert!(
        draft
            .campaign()
            .unwrap_err()
            .to_string()
            .contains("不能覆盖")
    );
}

#[test]
fn batch_add_accepts_common_separators_and_keeps_display_names() {
    let mut draft = CampaignEditor::default();
    draft.add_addresses(
        "Alice <alice@example.com>;bob@example.net\n\r\ncarol@example.net, david@example.net ",
    );
    assert_eq!(draft.recipients.len(), 4);
    assert_eq!(draft.recipients[0].email, "Alice <alice@example.com>");
    assert_eq!(draft.recipients[3].email, "david@example.net");
}

#[test]
fn editor_load_resolves_existing_templates_and_supports_disabling_html() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/campaign.json");
    let mut draft = CampaignEditor::load(&path).unwrap();
    assert!(draft.html.contains("<!doctype html>"));
    draft.html_enabled = false;
    let messages = draft.prepare().unwrap();
    assert!(
        messages
            .iter()
            .all(|email| email.html.is_none() && email.text.is_some())
    );
    draft.text_enabled = false;
    assert!(draft.prepare().is_err());
}

#[test]
fn config_save_rejects_paths_that_could_overwrite_a_companion_template() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("campaign.html");
    assert!(CampaignEditor::example().unwrap().save(&path).is_err());
    assert!(!path.exists());
}
