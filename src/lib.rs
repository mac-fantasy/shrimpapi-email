//! Cloudflare SMTP email submission with per-recipient Handlebars templates.
//!
//! Load a [`config::Campaign`], call [`prepare`] to validate the complete batch,
//! then submit it using [`CloudflareSmtp`] and [`send_all`].

pub mod client;
pub mod config;
pub mod editor;
pub mod output;
pub mod settings;
pub mod template;
pub mod templates;

pub use client::{CloudflareSmtp, MailTransport, SendReport, SendStatus, send_all};
pub use config::Campaign;
pub use template::{PreparedEmail, prepare, prepare_with_templates};
