//! FLASH-P2-06：本机自动配对 —— 让同一台电脑上的输入法不用输连接码就能连上本机 flashasr。
//!
//! 前台每次启动：保证有一个可用的「本机终端」（见 `pairing::Terminal::local`），把它的连接码写到
//! 当前用户目录 `<用户配置目录>/flashasr/local-pairing.json`：
//! - Windows `%APPDATA%\flashasr\`（用户配置目录默认只本人 + 系统可读）；macOS 设 0600；
//! - 连接码里的地址固定 `127.0.0.1:<端口>`，本机终端的 key 只接受回环来源 —— 文件被别的机器拿到也连不了；
//! - 终端被吊销 / 文件丢了 ⇒ 下次启动重建；端口改了 ⇒ 沿用 key、更新地址。
//!
//! 输入法（P3）读这个文件，按普通连接码解析即可。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::asr_shared::pairing::pair_uri;
use crate::ws::Shared;

/// 本机配对文件内容。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LocalPairing {
    /// 本机终端编号（排查用）。
    pub terminal_id: String,
    /// 连接码（`feiyin-asr://pair?v=1&lan=127.0.0.1:<端口>&key=…&fp=…`）。
    pub uri: String,
    pub port: u16,
}

pub fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("flashasr").join("local-pairing.json"))
}

/// 从连接码里取 key（`key=` 参数）。
fn key_of(uri: &str) -> Option<&str> {
    uri.split(['?', '&'])
        .find_map(|kv| kv.strip_prefix("key="))
        .filter(|k| !k.is_empty())
}

/// 启动时调用：保证本机终端可用、配对文件与当前端口 / 证书一致。失败只记日志（不影响对外服务）。
pub fn ensure(shared: &Shared, port: u16) {
    let Some(path) = path() else {
        log::warn!("[flashasr] 取不到用户配置目录，跳过本机自动配对");
        return;
    };
    let old: Option<LocalPairing> = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let old_key = old
        .as_ref()
        .and_then(|p| key_of(&p.uri))
        .map(str::to_string);
    let (terminal_id, key) = match shared.ensure_local_terminal(old_key.as_deref()) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("[flashasr] 本机终端准备失败：{e}");
            return;
        }
    };
    let want = LocalPairing {
        uri: pair_uri(
            &format!("127.0.0.1:{port}"),
            None,
            &key,
            &shared.fingerprint,
        ),
        terminal_id,
        port,
    };
    if old.as_ref() == Some(&want) {
        return;
    }
    if let Err(e) = write(&path, &want) {
        log::warn!("[flashasr] 写本机配对文件失败：{e}");
        return;
    }
    log::info!(
        "[flashasr] 本机配对文件已更新（终端 {}，端口 {port}）",
        want.terminal_id
    );
}

fn write(path: &std::path::Path, p: &LocalPairing) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let bytes = serde_json::to_vec_pretty(p).map_err(std::io::Error::other)?;
    crate::asr_shared::write_atomic(path, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_extracted_from_uri() {
        let u = "feiyin-asr://pair?v=1&lan=127.0.0.1:7860&key=ABCD-EFGH&fp=00";
        assert_eq!(key_of(u), Some("ABCD-EFGH"));
        assert_eq!(key_of("feiyin-asr://pair?v=1&lan=a&key=&fp=0"), None);
        assert_eq!(key_of("garbage"), None);
    }
}
