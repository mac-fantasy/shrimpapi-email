use std::{fs, path::Path};

use anyhow::{Context, Result};

use crate::template::{PreparedEmail, PreviewEntry};

pub fn export_previews(directory: &Path, messages: &[PreparedEmail]) -> Result<()> {
    fs::create_dir_all(directory)
        .with_context(|| format!("无法创建预览目录 {}", directory.display()))?;
    let mut manifest = Vec::new();
    for (index, email) in messages.iter().enumerate() {
        let stem = format!("{:04}", index + 1);
        let eml_file = format!("{stem}.eml");
        fs::write(directory.join(&eml_file), email.message.formatted())
            .context("无法写入 EML 预览")?;
        let mut entry = PreviewEntry {
            recipient: &email.recipient,
            subject: &email.subject,
            eml_file,
            html_file: None,
            text_file: None,
        };
        for (extension, body, target) in [
            ("html", &email.html, &mut entry.html_file),
            ("txt", &email.text, &mut entry.text_file),
        ] {
            if let Some(body) = body {
                let filename = format!("{stem}.{extension}");
                fs::write(directory.join(&filename), body).context("无法写入正文预览")?;
                *target = Some(filename);
            }
        }
        manifest.push(entry);
    }
    fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )
    .context("无法写入预览清单")?;
    Ok(())
}
