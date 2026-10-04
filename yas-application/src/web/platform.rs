use super::{Connection, GAME_TITLES};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::{path::PathBuf, process::Command};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONQUESTION, MB_TOPMOST, MB_YESNO,
};

const REGISTRY_KEY: &str = r"HKCU\Software\Classes\yas-scan";

pub fn authorize(connection: &Connection) -> bool {
    let message: Vec<u16> = format!("允许以下网站使用 YAS 扫描圣遗物吗？\n\n{}\n\n扫描会控制原神窗口的鼠标和键盘，并把扫描结果交给该网站。仅允许你信任的网站。", connection.origin)
        .encode_utf16().chain(Some(0)).collect();
    let title: Vec<u16> = "YAS 网页扫描授权".encode_utf16().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONQUESTION | MB_TOPMOST,
        ) == IDYES
    }
}

#[derive(Serialize)]
pub struct GameWindow {
    pub hwnd: isize,
    pub title: String,
}

pub fn windows() -> Vec<GameWindow> {
    yas::utils::iterate_window()
        .into_iter()
        .filter_map(|hwnd| {
            let title = yas::utils::get_window_title(hwnd)?;
            GAME_TITLES.contains(&title.trim()).then(|| GameWindow {
                hwnd: hwnd as isize,
                title,
            })
        })
        .collect()
}

fn installation_path() -> Result<PathBuf> {
    let directory = std::env::var_os("LOCALAPPDATA").context("未找到本机应用目录")?;
    Ok(PathBuf::from(directory)
        .join("YAS")
        .join("Web")
        .join("yas.exe"))
}

fn registry(args: &[&str]) -> Result<()> {
    let output = Command::new("reg.exe").args(args).output()?;
    if !output.status.success() {
        bail!(
            "注册唤起协议失败：{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

pub fn install(prefix: &[&str]) -> Result<()> {
    let destination = installation_path()?;
    let source = std::env::current_exe()?;
    std::fs::create_dir_all(destination.parent().unwrap())?;
    if source != destination {
        std::fs::copy(&source, &destination)
            .context("无法更新 YAS，请先关闭正在运行的 YAS 网页服务再安装")?;
        for file in std::fs::read_dir(source.parent().unwrap())? {
            let file = file?.path();
            if file.is_file()
                && file
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
            {
                let target = destination
                    .parent()
                    .unwrap()
                    .join(file.file_name().unwrap());
                if file != target {
                    std::fs::copy(file, target)
                        .context("无法复制运行库，请先关闭 YAS 服务再安装")?;
                }
            }
        }
    }
    let prefix = if prefix.is_empty() { "" } else { "genshin " };
    let command = format!("\"{}\" {prefix}--web-launch \"%1\"", destination.display());
    registry(&[
        "add",
        REGISTRY_KEY,
        "/ve",
        "/d",
        "URL:YAS Web Scanner",
        "/f",
    ])?;
    registry(&["add", REGISTRY_KEY, "/v", "URL Protocol", "/d", "", "/f"])?;
    registry(&[
        "add",
        &format!(r"{REGISTRY_KEY}\shell\open\command"),
        "/ve",
        "/d",
        &command,
        "/f",
    ])?;
    log::info!("网页唤起已安装。可以返回圣遗物网页，点击「启动并连接 YAS」。");
    Ok(())
}

pub fn uninstall() -> Result<()> {
    // Leave the binary in place if the service is still running; only remove our registration.
    let output = Command::new("reg.exe")
        .args(["query", REGISTRY_KEY, "/ve"])
        .output()?;
    if !output.status.success() {
        return Ok(());
    }
    if !String::from_utf8_lossy(&output.stdout).contains("URL:YAS Web Scanner") {
        bail!("yas-scan 协议已由其他程序注册，不会移除");
    }
    registry(&["delete", REGISTRY_KEY, "/f"])?;
    log::info!("网页唤起协议已移除。");
    Ok(())
}
