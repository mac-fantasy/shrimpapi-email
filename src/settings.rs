//! Versioned, atomic application storage outside the installed application.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::{SendReport, editor::CampaignEditor, templates::TemplateLibrary};

pub const APP_ID: &str = "com.shrimpapi.email";
pub const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Page {
    #[default]
    Compose,
    Templates,
    Recipients,
    Variables,
    Connection,
    Results,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub page: Page,
    pub edit_html: bool,
    pub preview_html: bool,
    pub selected_recipient: usize,
    pub selected_template: Option<u64>,
    pub config_path: Option<PathBuf>,
    pub bulk_addresses: String,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            page: Page::Compose,
            edit_html: true,
            preview_html: false,
            selected_recipient: 0,
            selected_template: Some(1),
            config_path: None,
            bulk_addresses: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub version: u32,
    pub draft: CampaignEditor,
    pub library: TemplateLibrary,
    pub timeout_secs: u64,
    pub ui: UiSettings,
    pub last_report: Option<SendReport>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            draft: CampaignEditor::example().expect("embedded example is valid"),
            library: TemplateLibrary::default(),
            timeout_secs: 30,
            ui: UiSettings::default(),
            last_report: None,
        }
    }
}

impl AppSettings {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut value: serde_json::Value =
            serde_json::from_slice(bytes).context("设置 JSON 无效")?;
        if !value.is_object()
            || !["version", "draft", "subject"]
                .iter()
                .any(|key| value.get(key).is_some())
        {
            bail!("文件不是有效的应用设置或旧版草稿");
        }
        let version = value
            .get("version")
            .map(|v| v.as_u64().context("设置版本格式无效"))
            .transpose()?
            .unwrap_or(0);
        if version > u64::from(SCHEMA_VERSION) {
            bail!("设置由更新版本的软件创建，请升级应用；原文件已保留");
        }
        // Version 0 was an unversioned editor snapshot. Preserve that draft while
        // filling new fields, rather than replacing it with bundled examples.
        if version == 0 {
            if value.get("draft").is_none() && value.get("subject").is_some() {
                value = serde_json::json!({ "draft": value });
            }
            if !value.is_object() {
                bail!("设置必须是 JSON 对象");
            }
            value["version"] = SCHEMA_VERSION.into();
        }
        let mut settings: Self = serde_json::from_value(value).context("设置结构无效")?;
        if version < 2 {
            // Replace the previous catalog once, including saved copies, as part
            // of the requested template reset. All other settings and drafts stay intact.
            settings.library = TemplateLibrary::default();
            settings.ui.selected_template = Some(1);
            settings.version = SCHEMA_VERSION;
        }
        settings.library.validate()?;
        settings.timeout_secs = settings.timeout_secs.clamp(1, 300);
        if settings
            .ui
            .selected_template
            .is_some_and(|id| settings.library.get(id).is_none())
        {
            settings.ui.selected_template = settings
                .library
                .templates
                .first()
                .map(|template| template.id);
        }
        Ok(settings)
    }

    pub fn export(&self, path: &Path) -> Result<()> {
        atomic_write(path, &serde_json::to_vec_pretty(self)?)
    }
}

#[derive(Debug, Clone)]
pub struct SettingsStore {
    pub directory: PathBuf,
}

pub struct LoadedSettings {
    pub settings: AppSettings,
    pub warning: Option<String>,
    pub needs_save: bool,
}

impl SettingsStore {
    pub fn system() -> Result<Self> {
        let dirs =
            ProjectDirs::from("com", "shrimpapi", "email").context("无法确定应用数据目录")?;
        Ok(Self::new(dirs.config_dir().to_path_buf()))
    }

    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
    pub fn path(&self) -> PathBuf {
        self.directory.join("settings.json")
    }
    pub fn backup_path(&self) -> PathBuf {
        self.directory.join("settings.backup.json")
    }
    pub fn window_path(&self) -> PathBuf {
        self.directory.join("window.ron")
    }

    fn preserve_upgrade_backup(&self, bytes: &[u8]) -> Result<bool> {
        let value: serde_json::Value = serde_json::from_slice(bytes)?;
        let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
        if version >= u64::from(SCHEMA_VERSION) {
            return Ok(false);
        }
        let path = self
            .directory
            .join(format!("settings.v{version}.backup.json"));
        if !path.exists() {
            atomic_write(&path, bytes)?;
        }
        Ok(true)
    }

    fn decode_stored(&self, bytes: &[u8]) -> Result<LoadedSettings> {
        let settings = AppSettings::decode(bytes)?;
        let migrated = self.preserve_upgrade_backup(bytes)?;
        Ok(LoadedSettings {
            settings,
            warning: migrated.then(|| {
                "模板库已替换为「新用户注册、模型上新、服务通知」，升级前设置已备份。".into()
            }),
            needs_save: migrated,
        })
    }

    pub fn load(&self) -> Result<LoadedSettings> {
        match fs::read(self.path()) {
            Ok(bytes) => match self.decode_stored(&bytes) {
                Ok(loaded) => Ok(loaded),
                Err(error) => {
                    // A future schema must never silently fall back and be overwritten.
                    if serde_json::from_slice::<serde_json::Value>(&bytes)
                        .ok()
                        .and_then(|v| v.get("version").and_then(|v| v.as_u64()))
                        .is_some_and(|v| v > u64::from(SCHEMA_VERSION))
                    {
                        return Err(error);
                    }
                    let mut loaded = self.decode_stored(
                        &fs::read(self.backup_path())
                            .context("设置损坏且没有可恢复的备份，原文件已保留")?,
                    )?;
                    // Retain the corrupt original for investigation before autosave.
                    atomic_write(&self.directory.join("settings.damaged.json"), &bytes)?;
                    loaded.warning = Some(format!("设置读取失败，已从备份恢复：{error:#}"));
                    loaded.needs_save = true;
                    Ok(loaded)
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if self.backup_path().exists() {
                    let mut loaded = self.decode_stored(&fs::read(self.backup_path())?)?;
                    loaded.warning = Some("设置文件缺失，已从备份恢复。".into());
                    loaded.needs_save = true;
                    Ok(loaded)
                } else {
                    Ok(LoadedSettings {
                        settings: AppSettings::default(),
                        warning: None,
                        needs_save: true,
                    })
                }
            }
            Err(error) => Err(error).context("无法读取应用设置"),
        }
    }

    pub fn save(&self, settings: &AppSettings) -> Result<()> {
        settings.library.validate()?;
        if settings.version != SCHEMA_VERSION {
            bail!("不能保存不兼容的设置版本");
        }
        if let Ok(bytes) = fs::read(self.path()) {
            if serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .and_then(|v| v.get("version").and_then(|v| v.as_u64()))
                .is_some_and(|v| v > u64::from(SCHEMA_VERSION))
            {
                bail!("磁盘设置已由更新版本写入，为避免数据丢失已停止保存");
            }
            // Back up only valid data; never replace a good backup with corrupt input.
            if AppSettings::decode(&bytes).is_ok() {
                self.preserve_upgrade_backup(&bytes)?;
                atomic_write(&self.backup_path(), &bytes)?;
            }
        }
        settings.export(&self.path())
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).with_context(|| format!("无法创建数据目录 {}", parent.display()))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("无法保存 {}", path.display()))?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

/// The secret is deliberately absent from AppSettings and all JSON exports.
pub trait CredentialStore {
    fn load(&self) -> Result<String>;
    fn save(&self, token: &str) -> Result<()>;
}

pub struct SystemCredentialStore;

impl CredentialStore for SystemCredentialStore {
    fn load(&self) -> Result<String> {
        match keyring::Entry::new(APP_ID, "cloudflare-api-token")?.get_password() {
            Ok(token) => Ok(token),
            Err(keyring::Error::NoEntry) => Ok(String::new()),
            Err(error) => Err(error).context("无法读取系统凭据库中的 API 令牌"),
        }
    }

    fn save(&self, token: &str) -> Result<()> {
        let entry = keyring::Entry::new(APP_ID, "cloudflare-api-token")?;
        if token.is_empty() {
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(error) => Err(error).context("无法清除已保存的 API 令牌"),
            }
        } else {
            entry
                .set_password(token)
                .context("无法将 API 令牌保存到系统凭据库")
        }
    }
}
