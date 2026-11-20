#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::process::ExitCode;

mod gui;

fn main() -> ExitCode {
    match gui::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            rfd::MessageDialog::new()
                .set_title("ShrimpAPI Email 启动失败")
                .set_description(format!("{error:#}"))
                .set_level(rfd::MessageLevel::Error)
                .set_buttons(rfd::MessageButtons::Ok)
                .show();
            ExitCode::FAILURE
        }
    }
}
