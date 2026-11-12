//! ASR-SERVER-491：内核、网关、设置界面三方共用的识别服务数据（平台中立，只依赖 std + serde + ring + rustls）。
//!
//! 各方用 `#[path]` 引入同一份源码（网关、设置界面是独立程序）。数据放可执行文件同目录
//!（与词库、使用统计同规则），**不进 `config.toml`**（设计 §8.8：用户升级可能整份替换配置）。

pub mod control;
pub mod models;
pub mod pairing;
pub mod remote;
pub mod settings;
pub mod tls_pin;

use std::path::PathBuf;

/// 数据目录 = 可执行文件所在目录（取不到 ⇒ 当前目录）。
pub fn data_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// FLASH-P0-511 / USER-DIM：某终端或用户的声纹存档 `<数据目录>/voiceprints/<安全名>.bin`（本机主人仍是 `voiceprint.bin`）。
/// - 终端编号是配对时生成的小写 base32；
/// - 用户 ID（`user_id`）是 1~12 位字母数字（字母开头、区分大小写）；
/// - 为保证 Windows（文件系统大小写不敏感）与 macOS 下 `Alice` 与 `alice` 绝不碰撞：
///   若 ID 全小写，文件名为 `<id>.bin`（与旧终端编号完全一致、零迁移成本）；
///   若含大写字母，文件名为 `<id_lower>_u<case_mask_hex>.bin`（位掩码精确标记大写位，100% 一一对应且无特殊字符）。
pub fn terminal_voiceprint_path(id_or_user: &str) -> Option<PathBuf> {
    let id = id_or_user.trim();
    if id.is_empty() || id.len() > 32 || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let mut mask = 0u32;
    for (i, c) in id.chars().enumerate() {
        if c.is_ascii_uppercase() {
            mask |= 1u32 << i;
        }
    }
    let filename = if mask == 0 {
        format!("{id}.bin")
    } else {
        format!("{}_u{:x}.bin", id.to_ascii_lowercase(), mask)
    };
    Some(data_dir().join("voiceprints").join(filename))
}

/// 按用户 ID（或回退终端编号）取声纹存档路径。
pub fn user_voiceprint_path(user_id: &str, fallback_terminal_id: &str) -> Option<PathBuf> {
    let uid = user_id.trim();
    if !uid.is_empty() {
        if let Some(p) = terminal_voiceprint_path(uid) {
            return Some(p);
        }
    }
    terminal_voiceprint_path(fallback_terminal_id)
}

/// FLASH-P0-511：吊销终端时删其声纹 —— 声纹是生物特征数据，不得在终端吊销后保留。
/// 两个吊销入口（网关在跑 ⇒ 网关控制通道；网关没跑 ⇒ 设置界面直接改配对文件）都调它。没有文件 ⇒ `Ok`。
pub fn purge_terminal_voiceprint(terminal_id: &str) -> std::io::Result<()> {
    let Some(p) = terminal_voiceprint_path(terminal_id) else {
        return Ok(());
    };
    match std::fs::remove_file(&p) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// 当前 Unix 秒。
pub fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 原子写：先写同目录临时文件再改名（写到一半断电不会留下半截文件）。
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod flash_p0_511_tests {
    use super::*;

    #[test]
    fn terminal_voiceprint_path_validates_id() {
        let p = terminal_voiceprint_path("abc234").expect("合规编号");
        assert!(p.ends_with(std::path::Path::new("voiceprints").join("abc234.bin")));
        for bad in ["", " ", "..", "a/b", r"a\b", "a.b", "a b"] {
            assert!(terminal_voiceprint_path(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn purge_missing_is_ok() {
        assert!(purge_terminal_voiceprint("flashp0none").is_ok());
        assert!(
            purge_terminal_voiceprint("../evil").is_ok(),
            "不合规编号 ⇒ 什么都不删"
        );
    }

    #[test]
    fn user_voiceprint_path_case_sensitive_and_safe() {
        let p1 = user_voiceprint_path("gavin01", "term01").expect("合规全小写用户");
        assert!(p1.ends_with(std::path::Path::new("voiceprints").join("gavin01.bin")));

        let p_gavin_upper = user_voiceprint_path("Gavin01", "term01").expect("合规大写用户");
        let p_gavin_lower = user_voiceprint_path("gavin01", "term01").expect("合规小写用户");
        assert_ne!(
            p_gavin_upper.to_string_lossy().to_lowercase(),
            p_gavin_lower.to_string_lossy().to_lowercase(),
            "Gavin01 与 gavin01 文件名在大小写不敏感下绝不相等"
        );

        let p_alice_upper = user_voiceprint_path("Alice", "term01").expect("合规大写用户");
        let p_alice_lower = user_voiceprint_path("alice", "term01").expect("合规小写用户");
        assert_ne!(
            p_alice_upper.to_string_lossy().to_lowercase(),
            p_alice_lower.to_string_lossy().to_lowercase(),
            "Alice 与 alice 文件名在大小写不敏感下绝不相等"
        );

        let p_prio = user_voiceprint_path("Gavin01", "term123").expect("合规用户优先");
        assert_eq!(p_prio, p_gavin_upper);

        let p_fallback = user_voiceprint_path("", "term123").expect("回退到终端ID");
        let p_term = terminal_voiceprint_path("term123").expect("终端路径");
        assert_eq!(p_fallback, p_term);

        let p_fallback_whitespace =
            user_voiceprint_path("   ", "term123").expect("空白回退到终端ID");
        assert_eq!(p_fallback_whitespace, p_term);
    }
}
