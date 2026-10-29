//! flashasr 管理界面（FLASH-P2b-03）。
//!
//! 与前台 `flashasr` 装在同一目录（数据目录 = 程序所在目录，`asr_shared::data_dir`）；前台托盘「打开管理界面」拉起本程序。
//! 单实例：再开一次 = 把已开的窗口提到前面。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[allow(dead_code)]
#[path = "../../src/asr_shared/mod.rs"]
mod asr_shared;
#[allow(dead_code)] // 写入接口只在引擎用
#[path = "../../src/usage_stats.rs"]
mod usage_stats;

mod llm;
mod server;

use tauri::Manager;

fn main() {
    // 进程级 rustls ring provider（asr_shared 的 TLS 工具要用；重复安装返回 Err 属正常）。
    let _ = rustls::crypto::ring::default_provider().install_default();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .invoke_handler(tauri::generate_handler![
            server::server_overview,
            server::server_set_enabled,
            server::server_save_wan,
            server::server_set_paused,
            server::server_add_terminal,
            server::server_revoke,
            server::server_reset_voiceprint,
            server::server_sessions,
            server::open_folder,
            server::app_info,
            llm::llm_get,
            llm::llm_save,
            llm::llm_test,
        ])
        .run(tauri::generate_context!())
        .expect("error while running flashasr-ui");
}
