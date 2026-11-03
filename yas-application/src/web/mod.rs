//! A restricted, loopback-only bridge for the artifact import page.
use anyhow::{bail, Result};
use clap::{Arg, ArgAction, ArgGroup, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use url::Url;

#[cfg(windows)]
mod job;
#[cfg(windows)]
mod platform;
#[cfg(windows)]
mod server;

pub const ADDRESS: &str = "127.0.0.1:32334";
pub const GAME_TITLES: &[&str] = &["原神", "Genshin Impact", "云·原神"];

pub fn augment_command(command: Command) -> Command {
    command
        .arg(
            Arg::new("serve")
                .long("serve")
                .action(ArgAction::SetTrue)
                .help("启动本机网页连接服务"),
        )
        .arg(
            Arg::new("web-launch")
                .long("web-launch")
                .value_name("URI")
                .hide(true),
        )
        .arg(
            Arg::new("install-web")
                .long("install-web")
                .action(ArgAction::SetTrue)
                .help("安装网页唤起协议"),
        )
        .arg(
            Arg::new("uninstall-web")
                .long("uninstall-web")
                .action(ArgAction::SetTrue)
                .help("移除网页唤起协议"),
        )
        .group(ArgGroup::new("web-mode").args([
            "serve",
            "web-launch",
            "install-web",
            "uninstall-web",
        ]))
}

pub fn handle(matches: &ArgMatches, child_prefix: &[&str]) -> Result<bool> {
    if !matches.get_flag("serve")
        && !matches.get_flag("install-web")
        && !matches.get_flag("uninstall-web")
        && matches.get_one::<String>("web-launch").is_none()
    {
        return Ok(false);
    }
    #[cfg(windows)]
    {
        if matches.get_flag("install-web") {
            platform::install(child_prefix)?;
        } else if matches.get_flag("uninstall-web") {
            platform::uninstall()?;
        } else {
            let launch = matches
                .get_one::<String>("web-launch")
                .map(|uri| parse_launch(uri))
                .transpose()?;
            server::serve(launch, child_prefix)?;
        }
        Ok(true)
    }
    #[cfg(not(windows))]
    {
        let _ = child_prefix;
        bail!("网页扫描目前仅支持 Windows");
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub origin: String,
    pub token: String,
}

pub fn validate_origin(origin: &str) -> Result<()> {
    let url = Url::parse(origin)?;
    if url.origin().ascii_serialization() != origin
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("无效的网站来源");
    }
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if url.scheme() != "https" && !(url.scheme() == "http" && local) {
        bail!("仅允许 HTTPS 网站或本机开发页面");
    }
    Ok(())
}

pub fn validate_token(token: &str) -> Result<()> {
    if token.len() != 64 || !token.bytes().all(|c| c.is_ascii_hexdigit()) {
        bail!("无效的连接令牌");
    }
    Ok(())
}

fn parse_launch(uri: &str) -> Result<Connection> {
    let url = Url::parse(uri)?;
    if url.scheme() != "yas-scan"
        || url.host_str() != Some("connect")
        || !matches!(url.path(), "" | "/")
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        bail!("无效的网页唤起地址");
    }
    let mut origin = None;
    let mut token = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "origin" if origin.is_none() => origin = Some(value.into_owned()),
            "token" if token.is_none() => token = Some(value.into_owned()),
            _ => bail!("无效的网页唤起参数"),
        }
    }
    let connection = Connection {
        origin: origin.ok_or_else(|| anyhow::anyhow!("缺少来源"))?,
        token: token.ok_or_else(|| anyhow::anyhow!("缺少令牌"))?,
    };
    validate_origin(&connection.origin)?;
    validate_token(&connection.token)?;
    Ok(connection)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScanOptions {
    pub hwnd: isize,
    pub min_star: u8,
    pub min_level: u8,
    pub number: u32,
}

impl ScanOptions {
    pub fn validate(&self) -> Result<()> {
        if self.hwnd <= 0
            || !(1..=5).contains(&self.min_star)
            || self.min_level > 20
            || self.number > 10000
        {
            bail!("扫描参数超出范围");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_requires_exact_origin_and_token() {
        let token = "a".repeat(64);
        assert!(parse_launch(&format!(
            "yas-scan://connect?origin=https%3A%2F%2Fexample.com&token={token}"
        ))
        .is_ok());
        for origin in [
            "http://example.com",
            "https://example.com/path",
            "https://user@example.com",
            "null",
        ] {
            assert!(validate_origin(origin).is_err());
        }
        assert!(validate_origin("http://localhost:8080").is_ok());
        assert!(parse_launch(&format!(
            "yas-scan://connect?origin=https%3A%2F%2Fexample.com&token={token}&token={token}"
        ))
        .is_err());
        assert!(validate_token("short").is_err());
    }
    #[test]
    fn scan_options_cannot_supply_arbitrary_arguments() {
        assert!(serde_json::from_str::<ScanOptions>(
            r#"{"hwnd":1,"minStar":5,"minLevel":0,"number":0,"argv":"cmd.exe"}"#
        )
        .is_err());
        assert!(ScanOptions {
            hwnd: 1,
            min_star: 5,
            min_level: 0,
            number: 0
        }
        .validate()
        .is_ok());
        assert!(ScanOptions {
            hwnd: 1,
            min_star: 6,
            min_level: 0,
            number: 0
        }
        .validate()
        .is_err());
    }
}
