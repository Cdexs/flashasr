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

fn main() {
    // FLASH-P1-04 再接：加载模型 ⇒ 登记 ⇒ 看护线程（IPC + 网关）⇒ 常驻。
}
