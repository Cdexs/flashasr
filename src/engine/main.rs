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

/// 日志写到程序旁边的 `flashasr.log`（无控制台）；`-debug` ⇒ Debug 级，否则 Info 级。
/// 日志文件打不开 ⇒ 退回标准错误（开发时从终端启动也能看到）。
fn init_logging(debug: bool) {
    let level = if debug {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    let path = asr_shared::data_dir().join("flashasr.log");
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

/// flashasr 本身就是识别服务 ⇒ 总开关缺省开（首次运行写入设置文件）；用户在设置文件里关掉则尊重。
/// P2 起总开关由管理界面管理。
fn ensure_service_enabled_by_default() {
    let path = asr_shared::settings::ServiceSettings::path();
    if path.exists() {
        return;
    }
    let mut s = asr_shared::settings::ServiceSettings::load();
    s.enabled = true;
    match s.save() {
        Ok(()) => log::info!("[flashasr] first run: service switch defaulted to on"),
        Err(e) => log::warn!("[flashasr] failed to write default service settings: {e}"),
    }
}

/// `--add-terminal`：向正在运行的网关要一个新终端，把连接码打到标准输出（P2 管理界面之前的无界面配对方式）。
/// 连接码含一次性明文 key，只打一次、不落盘。网关没在运行 ⇒ 退出码 2。
fn add_terminal_cli() -> i32 {
    use asr_shared::control::{self, Request};
    match control::call(&Request::AddTerminal) {
        Ok(r) if r.error.is_empty() => match r.added {
            Some(a) => {
                println!("{}", a.uri);
                0
            }
            None => {
                eprintln!("gateway returned no terminal");
                1
            }
        },
        Ok(r) => {
            eprintln!("add terminal failed: {}", r.error);
            1
        }
        Err(e) => {
            eprintln!("flashasr gateway not running: {e}");
            2
        }
    }
}

fn main() {
    // 进程级 rustls ring provider（任何 TLS 使用之前；重复安装返回 Err 属正常）。
    let _ = rustls::crypto::ring::default_provider().install_default();
    if std::env::args().any(|a| a == "--add-terminal") {
        std::process::exit(add_terminal_cli());
    }
    let debug = std::env::args().any(|a| a == "-debug" || a == "--debug");
    init_logging(debug);
    log::info!("flashasr-core {} starting", env!("CARGO_PKG_VERSION"));

    let runtime_config = Arc::new(RwLock::new(AppConfig::load().unwrap_or_default()));
    let config = clone_runtime_config(&runtime_config);
    let model_dir = transcription::model_dir();

    // 本地实时双模型 + 对齐器（构造成功即登记给远程会话：`transcription::REMOTE_ENGINES`）。
    // 不预先注入词库：词库每个终端单独，随每段识别的 `vocabulary` 带来（FLASH-P0-509）。
    // 引擎须常驻 —— `Transcriber` 释放时注销登记。加载失败 ⇒ 记日志、服务照常起（会话回 `not_ready`）。
    let t0 = std::time::Instant::now();
    let _engines = match transcription::Transcriber::new(
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
    ) {
        Ok(t) => {
            log::info!(
                "[flashasr] local realtime engines loaded in {:.1}s ({})",
                t0.elapsed().as_secs_f32(),
                model_dir.display()
            );
            Some(t)
        }
        Err(e) => {
            log::error!(
                "[flashasr] local realtime engines failed to load from {}: {e:#}",
                model_dir.display()
            );
            None
        }
    };

    ensure_service_enabled_by_default();
    // 看护线程：每秒看设置文件，开 ⇒ 起本机 IPC + 拉起网关 `flashasr-gateway`。
    asr_service::supervisor::spawn(Arc::clone(&runtime_config));

    loop {
        std::thread::park();
    }
}
