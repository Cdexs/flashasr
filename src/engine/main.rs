//! flashasr-core：识别服务内核（无界面）。
//!
//! FLASH-P1：从 voice-ime（`7c1e6ae`）搬来的本地实时识别引擎 + 识别服务内核侧。职责：
//! 加载本地实时模型（SenseVoice 预览 + Qwen3-ASR 1.7B 精解 + 强制对齐）⇒ 登记给远程会话 ⇒
//! 看护线程起本机 IPC 并拉起网关 `flashasr-gateway` ⇒ 常驻。

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
// 搬来的代码里有只给输入法用的部分（P1-05 起逐步删）。
#![allow(dead_code)]

#[path = "../asr_service/mod.rs"]
mod asr_service;
#[path = "../asr_shared/mod.rs"]
mod asr_shared;
#[path = "../config/mod.rs"]
mod config;
#[path = "../homophone/mod.rs"]
mod homophone;
#[path = "../i18n.rs"]
mod i18n;
#[path = "../itn.rs"]
mod itn;
#[path = "../llm/mod.rs"]
mod llm;
#[path = "../localrt_session.rs"]
mod localrt_session;
#[path = "../punctuation/mod.rs"]
mod punctuation;
#[path = "../scene/mod.rs"]
mod scene;
#[path = "../text_normalizer.rs"]
mod text_normalizer;
#[path = "../transcription/mod.rs"]
mod transcription;
#[path = "../translation/mod.rs"]
mod translation;
#[path = "../usage_stats.rs"]
mod usage_stats;
#[path = "../wordbook/mod.rs"]
mod wordbook;

// FLASH-P1-03：从 voice-ime `main.rs` 复制的识别核心依赖项（精解窗口、预览回灌合成、事件类型、标点服务……）。
// `include!` 进 crate 根：与 voice-ime 同在根上，子模块的 `use super::*` / 私有字段访问逐位同语义。
include!("glue.rs");

/// 读「本文件源码」的护栏测试读这份合并源码（与 voice-ime 同口径：搬出部分 + 本文件 + 复制来的核心项）。
#[cfg(test)]
const MAIN_RS_SRC: &str = concat!(
    include_str!("../localrt_session.rs"),
    "\n",
    include_str!("glue.rs"),
    "\n",
    include_str!("main.rs")
);

#[allow(unused_imports)]
use anyhow::{anyhow, Result};
#[allow(unused_imports)]
use config::AppConfig;
#[allow(unused_imports)]
use std::path::Path;
#[allow(unused_imports)]
use std::sync::{
    atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering},
    Arc, Mutex, RwLock,
};
#[allow(unused_imports)]
use std::thread::{self, JoinHandle};
#[allow(unused_imports)]
use std::time::Duration;

/// 日志写到程序旁边的 `flashasr-engine.log`（无控制台）；`-debug` ⇒ Debug 级，否则 Info 级。
/// 日志文件打不开 ⇒ 退回标准错误。
fn init_logging(debug: bool) {
    let level = if debug {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    let path = asr_shared::data_dir().join("flashasr-engine.log");
    let mut b = env_logger::Builder::new();
    b.filter_level(level)
        .format_timestamp(Some(env_logger::TimestampPrecision::Millis));
    if let Ok(f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        b.target(env_logger::Target::Pipe(Box::new(f)));
    }
    b.init();
}

/// 前台交给引擎的 IPC 口令（环境变量，不上命令行；读完即清）。
const TOKEN_ENV: &str = "FLASHASR_TOKEN";
/// 引擎告诉前台 IPC 端点的那一行（标准输出第一行）。
const ENDPOINT_PREFIX: &str = "FLASHASR_ENDPOINT ";
/// FLASH-P2-04：引擎状态行（标准输出，JSON），前台据此回 `/v1/status`。
const STATUS_PREFIX: &str = "FLASHASR_STATUS ";

/// FLASH-P2b-01：本地实时模型加载结果 `(已加载, 失败原因)`（重载配置后重报状态要带上它）。
static LOCAL_STATE: Mutex<(bool, String)> = Mutex::new((false, String::new()));

/// 「配了大模型」= 开关开且地址、密钥都填了（缺省配置开关是开的但没填，不能算可用）。
fn llm_configured(config: &AppConfig) -> bool {
    config.llm.enabled
        && !config.llm.api_url.trim().is_empty()
        && !config.llm.api_key.trim().is_empty()
}

/// 向前台报一次状态：本地实时模型是否加载完（失败则带原因）、服务端是否配好大模型（输出优化 / 大模型翻译可用）。
fn report_status(llm: bool) {
    use std::io::Write;
    let (local_realtime, load_error) = LOCAL_STATE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    let line = serde_json::json!({
        "local_realtime": local_realtime,
        "llm": llm,
        "load_error": load_error,
    });
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{STATUS_PREFIX}{line}").and_then(|_| out.flush());
}

fn set_local_state(loaded: bool, error: String) {
    *LOCAL_STATE.lock().unwrap_or_else(|p| p.into_inner()) = (loaded, error);
}

/// FLASH-P2b-01：管理界面改了 `config.toml`（大模型统一配置等）⇒ 重读并替换运行时配置。
/// 新会话立即用新配置；进行中的会话用的是开始时的副本，不受影响。读失败 ⇒ 保持原配置。
pub(crate) fn reload_config(runtime_config: &Arc<RwLock<AppConfig>>) {
    match AppConfig::load() {
        Ok(c) => {
            let llm = llm_configured(&c);
            *runtime_config.write().unwrap_or_else(|p| p.into_inner()) = c;
            log::info!("[flashasr] config reloaded (llm configured: {llm})");
            report_status(llm);
        }
        Err(e) => log::warn!("[flashasr] config reload failed, keeping current: {e:#}"),
    }
}

/// 加载本地实时双模型 + 对齐器（构造成功即登记给远程会话：`transcription::REMOTE_ENGINES`），然后常驻。
/// 不预先注入词库：词库每个终端单独，随每段识别的 `vocabulary` 带来（FLASH-P0-509）。
/// `Transcriber` 释放时注销登记 ⇒ 本线程持有它直到进程结束。加载失败 ⇒ 记日志，会话回 `not_ready`。
fn load_local_realtime(config: AppConfig) {
    report_status(llm_configured(&config));
    let model_dir = transcription::model_dir();
    let t0 = std::time::Instant::now();
    let engines = transcription::Transcriber::new(
        &model_dir,
        true,
        "auto".to_string(),
        transcription::AsrModel::LocalRealtime,
        None,
        &config.audio.asr_online_api_key,
        &config.audio.asr_online_url,
        &config.audio.asr_online_model,
        config.audio.asr_online_max_sentence_silence,
        config.audio.asr_online_semantic_punctuation_enabled,
    );
    let _engines = match engines {
        Ok(t) => {
            log::info!(
                "[flashasr] local realtime engines loaded in {:.1}s ({})",
                t0.elapsed().as_secs_f32(),
                model_dir.display()
            );
            set_local_state(true, String::new());
            report_status(llm_configured(&config));
            t
        }
        Err(e) => {
            log::error!(
                "[flashasr] local realtime engines failed to load from {}: {e:#}",
                model_dir.display()
            );
            set_local_state(false, format!("{e:#}"));
            report_status(llm_configured(&config));
            return;
        }
    };
    loop {
        std::thread::park();
    }
}

/// FLASH-P2-01：引擎由前台 `flashasr` 拉起：
/// 1. 先开本机 IPC（口令来自前台），把端点写到标准输出第一行 ⇒ 前台据此连接；
/// 2. 后台加载模型（加载完之前会话回 `not_ready`，前台不必等）；
/// 3. 标准输入是前台握着的管道 —— 读到 EOF（前台退出 / 崩溃）⇒ 引擎随之退出，不留孤儿进程。
fn main() {
    // 进程级 rustls ring provider（任何 TLS 使用之前；重复安装返回 Err 属正常）。
    let _ = rustls::crypto::ring::default_provider().install_default();
    let debug = std::env::args().any(|a| a == "-debug" || a == "--debug");
    init_logging(debug);
    log::info!("flashasr-engine {} starting", env!("CARGO_PKG_VERSION"));

    let Ok(token) = std::env::var(TOKEN_ENV) else {
        log::error!(
            "[flashasr] {TOKEN_ENV} missing: the engine is started by `flashasr`, not directly"
        );
        std::process::exit(2);
    };
    std::env::remove_var(TOKEN_ENV);

    let runtime_config = Arc::new(RwLock::new(AppConfig::load().unwrap_or_default()));
    let svc = match asr_service::AsrService::start(Arc::clone(&runtime_config), token) {
        Ok(s) => s,
        Err(e) => {
            log::error!("[flashasr] IPC start failed: {e}");
            std::process::exit(1);
        }
    };
    {
        use std::io::Write;
        let mut out = std::io::stdout().lock();
        if writeln!(out, "{ENDPOINT_PREFIX}{}", svc.endpoint())
            .and_then(|_| out.flush())
            .is_err()
        {
            log::error!("[flashasr] cannot report IPC endpoint to front; exiting");
            std::process::exit(1);
        }
    }

    let config = clone_runtime_config(&runtime_config);
    if let Err(e) = std::thread::Builder::new()
        .name("engine-load".into())
        .spawn(move || load_local_realtime(config))
    {
        log::error!("[flashasr] model loader thread failed: {e}");
    }

    // 前台握着标准输入：读到 EOF / 出错 ⇒ 前台不在了 ⇒ 退出。
    let mut sink = [0u8; 64];
    loop {
        use std::io::Read;
        match std::io::stdin().read(&mut sink) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
    }
    log::info!("[flashasr] front gone; engine exiting");
    svc.stop();
    std::process::exit(0);
}
