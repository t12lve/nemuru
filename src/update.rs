//! Mise à jour depuis la dernière release GitHub.
//! Une version plus récente télécharge l'installeur, puis le lance une fois le compte terminé.

use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::fs::File;
use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

const REPO: &str = "t12lve/nemuru";
const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
pub struct Ready {
    pub version: String,
    pub installer: PathBuf,
}

pub fn check_in_background() -> Receiver<Ready> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("mise-a-jour".into())
        .spawn(move || match look_for_update() {
            Ok(Some(ready)) => {
                let _ = tx.send(ready);
            }
            Ok(None) => {}
            Err(err) => log::warn!("mise à jour : {err}"),
        })
        .ok();
    rx
}

pub fn install(ready: &Ready) -> Result<()> {
    let exe = std::env::current_exe()?;
    let script = ready.installer.with_extension("cmd");
    let body = format!(
        "@echo off\r\nping -n 4 127.0.0.1 >nul\r\n\"{}\" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART\r\nif errorlevel 1 exit /b 1\r\nstart \"\" \"{}\"\r\n",
        ready.installer.display(),
        exe.display()
    );
    let mut file = File::create(&script)?;
    file.write_all(body.as_bytes())?;
    file.flush()?;
    drop(file);
    Command::new("cmd")
        .args(["/C", &script.to_string_lossy()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .spawn()?;
    Ok(())
}

fn look_for_update() -> Result<Option<Ready>> {
    let response = agent()
        .get(&format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", &format!("Nemuru/{CURRENT}"))
        .call()
        .map_err(|err| anyhow!("{err}"))?;
    let body = response.into_string()?;
    let release: Release = serde_json::from_str(&body)?;
    if release.prerelease || !is_newer(&release.tag_name, CURRENT) {
        return Ok(None);
    }
    let asset = release
        .assets
        .into_iter()
        .find(|asset| {
            let name = asset.name.to_ascii_lowercase();
            name.starts_with("nemuru_setup_") && name.ends_with(".exe")
        })
        .ok_or_else(|| anyhow!("installeur absent de la release"))?;
    let version = release.tag_name.trim().trim_start_matches('v').to_string();
    let path = std::env::temp_dir().join(format!("Nemuru_Setup_v{version}.exe"));
    let mut file = File::create(&path)?;
    let response = agent()
        .get(&asset.browser_download_url)
        .set("User-Agent", &format!("Nemuru/{CURRENT}"))
        .call()
        .map_err(|err| anyhow!("{err}"))?;
    std::io::copy(&mut response.into_reader(), &mut file)?;
    file.flush()?;
    Ok(Some(Ready {
        version,
        installer: path,
    }))
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(120))
        .build()
}

fn is_newer(latest: &str, current: &str) -> bool {
    parts(latest) > parts(current)
}

fn parts(version: &str) -> [u64; 3] {
    let mut numbers = [0; 3];
    for (index, piece) in version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .take(3)
        .enumerate()
    {
        numbers[index] = piece.parse().unwrap_or(0);
    }
    numbers
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn a_later_release_is_newer() {
        assert!(is_newer("v1.2.0", "1.1.0"));
        assert!(is_newer("1.2.1", "1.2.0"));
        assert!(!is_newer("1.2.0", "1.2.0"));
        assert!(!is_newer("v1.1.0", "1.2.0"));
    }
}
