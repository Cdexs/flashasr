//! FLASH-P2b-03：服务页 + 终端页后端（由 voice-ime 设置界面「识别服务」页 `src-tauri/src/asr_server.rs` 改来）。
//!
//! - 启动 / 停止 = 写 `asr-service.json` 的总开关（前台 1 秒内跟上：自重启）。
//! - 暂停 / 添加终端 / 撤销 / 重置声纹 / 终端列表：前台在运行 ⇒ 经本机控制通道（运行期间终端表只由前台写）；
//!   前台没在运行 ⇒ 撤销 / 重置声纹直接改文件（此时没有别的写者），添加终端要求服务在运行（连接码要带证书指纹）。
//! - 用量数字：只读 `usage-stats.sqlite`（引擎写）。
//! - 错误一律回错误码，界面按码取文案（不把内部错误原文上屏）。

use serde::Serialize;

use crate::asr_shared::control::{self, Added, Request, Status, TerminalRow as CtlRow};
use crate::asr_shared::pairing::{self, TerminalStore};
use crate::asr_shared::settings::ServiceSettings;
use crate::asr_shared::{self, purge_terminal_voiceprint, terminal_voiceprint_path};
use crate::usage_stats as stats;

const ERR_NOT_RUNNING: &str = "not_running";
const ERR_IO: &str = "io";
const ERR_NOT_FOUND: &str = "not_found";

#[derive(Serialize)]
pub struct TerminalRow {
    pub id: String,
    pub device_name: String,
    pub local: bool,
    pub bound: bool,
    pub created_ts: i64,
    pub last_login_ts: i64,
    pub last_logout_ts: i64,
    /// `lan` / `wan`：在线取当前连接，离线取最近一次。
    pub net: String,
    pub online: bool,
    pub has_voiceprint: bool,
    pub week_speech_ms: u64,
    pub week_words: u64,
    pub total_speech_ms: u64,
    pub total_words: u64,
}

#[derive(Serialize)]
pub struct Overview {
    pub enabled: bool,
    pub port: u16,
    pub wan_host: String,
    pub wan_port: u16,
    /// 前台在运行（控制通道连得上）。
    pub running: bool,
    pub paused: bool,
    pub engine_online: bool,
    pub local_realtime_ready: bool,
    pub llm_configured: bool,
    /// 本地实时模型加载失败的原因（引擎原文，界面放在「详情」里给排查用；空 = 没失败）。
    pub load_error: String,
    /// 模型目录（界面显示路径 + 「打开文件夹」）。
    pub model_dir: String,
    /// 内网 `地址:端口`（自动）。
    pub lan: String,
    pub fingerprint: String,
    pub online_count: usize,
    pub paired_count: usize,
    pub terminals: Vec<TerminalRow>,
}

fn status() -> Option<Status> {
    control::call(&Request::Status).ok().and_then(|r| r.status)
}

/// 前台在运行 ⇒ 控制通道给的列表（带在线状态）；否则直接读终端表（全部离线）。
/// 从没连过的普通 key（添加了还没去连）不进列表：过期即清，未过期只是在等扫码。本机终端总是列出。
fn terminal_rows(running: bool) -> Vec<CtlRow> {
    let from_ctl = running
        .then(|| control::call(&Request::Terminals).ok())
        .flatten()
        .and_then(|r| r.terminals);
    let rows = from_ctl.unwrap_or_else(|| {
        TerminalStore::load()
            .terminals
            .iter()
            .filter(|t| !t.revoked)
            .map(|t| CtlRow {
                terminal_id: t.id.clone(),
                device_name: t.device_name.clone(),
                bound: t.bound(),
                local: t.local,
                created_ts: t.created_ts,
                last_login_ts: t.last_login_ts,
                last_logout_ts: t.last_logout_ts,
                last_net: t.last_net.clone(),
                online: false,
                has_voiceprint: terminal_voiceprint_path(&t.id).is_some_and(|p| p.exists()),
            })
            .collect()
    });
    rows.into_iter().filter(|t| t.bound || t.local).collect()
}

#[tauri::command]
pub fn server_overview() -> Result<Overview, String> {
    let settings = ServiceSettings::load();
    let st = status();
    let usage = stats::terminal_usage().unwrap_or_default();
    let online = st.as_ref().map(|s| s.online.as_slice()).unwrap_or(&[]);
    let mut terminals: Vec<TerminalRow> = terminal_rows(st.is_some())
        .into_iter()
        .map(|t| {
            let on = online.iter().find(|o| o.terminal_id == t.terminal_id);
            let u = usage.iter().find(|u| u.terminal_id == t.terminal_id);
            TerminalRow {
                net: on.map(|o| o.net.clone()).unwrap_or(t.last_net),
                online: t.online,
                week_speech_ms: u.map_or(0, |u| u.week.speech_ms),
                week_words: u.map_or(0, |u| u.week.words),
                total_speech_ms: u.map_or(0, |u| u.total.speech_ms),
                total_words: u.map_or(0, |u| u.total.words),
                id: t.terminal_id,
                device_name: t.device_name,
                local: t.local,
                bound: t.bound,
                created_ts: t.created_ts,
                last_login_ts: t.last_login_ts,
                last_logout_ts: t.last_logout_ts,
                has_voiceprint: t.has_voiceprint,
            }
        })
        .collect();
    // 本机在最前；其余在线在前，再按最近登录。
    terminals.sort_by(|a, b| {
        b.local
            .cmp(&a.local)
            .then(b.online.cmp(&a.online))
            .then(b.last_login_ts.cmp(&a.last_login_ts))
    });
    let s = st.as_ref();
    Ok(Overview {
        enabled: settings.enabled,
        port: settings.port,
        wan_host: settings.wan_host.clone(),
        wan_port: settings.wan_port,
        running: s.is_some(),
        paused: s.is_some_and(|s| s.paused),
        engine_online: s.is_some_and(|s| s.engine_online),
        local_realtime_ready: s.is_some_and(|s| s.local_realtime_ready),
        llm_configured: s.is_some_and(|s| s.llm_configured),
        load_error: s.map(|s| s.load_error.clone()).unwrap_or_default(),
        model_dir: asr_shared::data_dir().join("models").display().to_string(),
        lan: settings.lan_endpoint().unwrap_or_default(),
        fingerprint: s
            .map(|s| s.fingerprint.clone())
            .or_else(pairing::cert_fingerprint)
            .unwrap_or_default(),
        online_count: terminals.iter().filter(|t| t.online).count(),
        paired_count: terminals.len(),
        terminals,
    })
}

/// 启动 / 停止（总开关）。前台 1 秒内跟上。
#[tauri::command]
pub fn server_set_enabled(enabled: bool) -> Result<(), String> {
    let mut s = ServiceSettings::load();
    s.enabled = enabled;
    s.save().map_err(|_| ERR_IO.to_string())
}

/// 保存外网地址（选填；端口 0 = 与本机端口相同）。
#[tauri::command]
pub fn server_save_wan(wan_host: String, wan_port: u16) -> Result<(), String> {
    let mut s = ServiceSettings::load();
    s.wan_host = wan_host.trim().to_string();
    s.wan_port = wan_port;
    s.save().map_err(|_| ERR_IO.to_string())
}

#[tauri::command]
pub fn server_set_paused(paused: bool) -> Result<(), String> {
    let req = if paused {
        Request::Pause
    } else {
        Request::Resume
    };
    control::call(&req).map_err(|_| ERR_NOT_RUNNING.to_string())?;
    Ok(())
}

#[derive(Serialize)]
pub struct AddedTerminal {
    #[serde(flatten)]
    pub added: Added,
    /// 二维码（SVG 文本）。
    pub qr_svg: String,
    pub fingerprint: String,
}

#[tauri::command]
pub fn server_add_terminal() -> Result<AddedTerminal, String> {
    let resp = control::call(&Request::AddTerminal).map_err(|_| ERR_NOT_RUNNING.to_string())?;
    let added = match (resp.error.is_empty(), resp.added) {
        (true, Some(a)) => a,
        _ => return Err(ERR_IO.to_string()),
    };
    let qr_svg = qrcode::QrCode::new(added.uri.as_bytes())
        .map(|q| {
            q.render::<qrcode::render::svg::Color>()
                .min_dimensions(220, 220)
                .quiet_zone(true)
                .build()
        })
        .unwrap_or_default();
    Ok(AddedTerminal {
        added,
        qr_svg,
        fingerprint: status().map(|s| s.fingerprint).unwrap_or_default(),
    })
}

#[tauri::command]
pub fn server_revoke(terminal_id: String) -> Result<(), String> {
    match control::call(&Request::Revoke {
        terminal_id: terminal_id.clone(),
    }) {
        Ok(r) if r.error.is_empty() => Ok(()),
        Ok(_) => Err(ERR_NOT_FOUND.to_string()),
        // 前台没在运行：直接改文件（此时没有别的写者）。
        Err(_) => {
            let mut st = TerminalStore::load();
            if !st.revoke(&terminal_id) {
                return Err(ERR_NOT_FOUND.to_string());
            }
            st.save().map_err(|_| ERR_IO.to_string())?;
            // 吊销 = 连该终端的声纹一起删。
            purge_terminal_voiceprint(&terminal_id).map_err(|_| ERR_IO.to_string())
        }
    }
}

/// 重置某终端的声纹（之后从零重新累积）。
#[tauri::command]
pub fn server_reset_voiceprint(terminal_id: String) -> Result<(), String> {
    match control::call(&Request::ResetVoiceprint {
        terminal_id: terminal_id.clone(),
    }) {
        Ok(r) if r.error.is_empty() => Ok(()),
        Ok(_) => Err(ERR_NOT_FOUND.to_string()),
        // 前台没在运行 ⇒ 引擎也不在：直接删档。
        Err(_) => purge_terminal_voiceprint(&terminal_id).map_err(|_| ERR_IO.to_string()),
    }
}

#[derive(Serialize)]
pub struct SessionRow {
    pub login_ts: i64,
    pub logout_ts: Option<i64>,
}

/// 某终端最近 20 次登录 / 退出。
#[tauri::command]
pub fn server_sessions(terminal_id: String) -> Result<Vec<SessionRow>, String> {
    stats::terminal_sessions(&terminal_id, 20)
        .map(|v| {
            v.into_iter()
                .map(|s| SessionRow {
                    login_ts: s.login_ts,
                    logout_ts: s.logout_ts,
                })
                .collect()
        })
        .map_err(|_| ERR_IO.to_string())
}

/// 打开模型目录 / 程序目录（日志在程序目录：`flashasr.log`、`flashasr-engine.log`）。
#[tauri::command]
pub fn open_folder(kind: String) -> Result<(), String> {
    let dir = asr_shared::data_dir();
    let dir = if kind == "models" {
        dir.join("models")
    } else {
        dir
    };
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer").arg(&dir).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(&dir).spawn();
    r.map(|_| ()).map_err(|_| ERR_IO.to_string())
}

#[derive(Serialize)]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir: asr_shared::data_dir().display().to_string(),
    }
}
