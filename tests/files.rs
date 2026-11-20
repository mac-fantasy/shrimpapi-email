use std::fs;

use serde_json::{Value, json};
use shrimpapi_email::{
    config::load_recipients,
    editor::{CampaignEditor, RecipientRow},
    output::export_previews,
};

#[test]
fn exported_previews_keep_recipient_mapping_and_personalized_bodies() {
    let directory = tempfile::tempdir().unwrap();
    let messages = CampaignEditor::example().unwrap().prepare().unwrap();
    let output = directory.path().join("previews");
    export_previews(&output, &messages).unwrap();
    let manifest: Vec<Value> =
        serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest.len(), 3);
    for (entry, message) in manifest.iter().zip(&messages) {
        assert_eq!(entry["recipient"], message.recipient);
        assert_eq!(entry["subject"], message.subject);
        assert_eq!(
            fs::read(output.join(entry["eml_file"].as_str().unwrap())).unwrap(),
            message.message.formatted()
        );
        assert_eq!(
            fs::read_to_string(output.join(entry["html_file"].as_str().unwrap())).unwrap(),
            message.html.as_deref().unwrap()
        );
        assert_eq!(
            fs::read_to_string(output.join(entry["text_file"].as_str().unwrap())).unwrap(),
            message.text.as_deref().unwrap()
        );
    }
}

#[test]
fn imported_recipients_keep_names_typed_variables_and_personal_overrides() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("recipients.json");
    fs::write(
        &path,
        json!([
            {"email": "alice@example.com", "name": "Imported Alice", "variables": {
                "plan": "导入方案", "coupon": "IMPORT40", "show_coupon": false
            }},
            "bob@example.net"
        ])
        .to_string(),
    )
    .unwrap();
    let mut draft = CampaignEditor::example().unwrap();
    draft.recipients = load_recipients(&path)
        .unwrap()
        .into_iter()
        .map(RecipientRow::from_recipient)
        .collect();
    let messages = draft.prepare().unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[0].subject,
        "Imported Alice，欢迎使用 ShrimpAPI · 导入方案"
    );
    assert!(!messages[0].html.as_ref().unwrap().contains("IMPORT40"));
    assert_eq!(messages[1].recipient, "bob@example.net");
    assert_eq!(draft.campaign().unwrap().recipients.len(), 2);
}
