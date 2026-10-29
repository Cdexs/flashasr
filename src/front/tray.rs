//! FLASH-P2b-02：托盘（Windows）。
//!
//! - 菜单：打开管理界面 / 暂停接入（勾选）/ 退出；双击图标 = 打开管理界面。
//! - 图标提示每 2 秒刷新：服务已关闭 / 引擎启动中 / 模型加载中 / 模型加载失败 / 已暂停 / 运行中（N 个终端在线）。
//! - 独立线程跑自己的消息循环：菜单弹出时是模态的，只卡这条线程，不影响识别与对外服务。
//!   （voice-ime 的托盘冻结问题来自菜单与热键共用一条线程；这里没有热键。）
//! - 管理界面 `flashasr-ui` 在本程序旁边；它自己保证单实例（再开一次 = 把已开的窗口提到前面）。
//! - 菜单文字跟系统界面语言（简体 / 繁體 / English）。
//!
//! macOS：本期不出 macOS 包，菜单栏图标要在主线程跑事件循环，届时另做（见 MACOS-HANDOFF）。

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostThreadMessageW, SetTimer, TranslateMessage, MSG, WM_QUIT,
    WM_TIMER,
};

use crate::ws::Shared;

/// 服务在跑时由 `run()` 填上；没填 = 总开关关着。
pub static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();

const UI_EXE: &str = "flashasr-ui.exe";
const REFRESH_MS: u32 = 2000;
/// 与 `build.rs` 里 winres `set_icon` 的资源号一致（winres 缺省用 1）。
const ICON_RESOURCE_ID: u16 = 1;

static THREAD_ID: AtomicU32 = AtomicU32::new(0);
static DONE: AtomicBool = AtomicBool::new(false);

struct Strings {
    open: &'static str,
    pause: &'static str,
    exit: &'static str,
    off: &'static str,
    engine_starting: &'static str,
    loading: &'static str,
    load_failed: &'static str,
    paused: &'static str,
    /// `{n}` = 在线终端数。
    running: &'static str,
}

const ZH_HANS: Strings = Strings {
    open: "打开管理界面",
    pause: "暂停接入",
    exit: "退出",
    off: "识别服务已关闭",
    engine_starting: "识别引擎启动中",
    loading: "模型加载中",
    load_failed: "模型加载失败，请打开管理界面查看",
    paused: "已暂停接入",
    running: "运行中 · {n} 个终端在线",
};

const ZH_HANT: Strings = Strings {
    open: "開啟管理介面",
    pause: "暫停接入",
    exit: "結束",
    off: "辨識服務已關閉",
    engine_starting: "辨識引擎啟動中",
    loading: "模型載入中",
    load_failed: "模型載入失敗，請開啟管理介面查看",
    paused: "已暫停接入",
    running: "執行中 · {n} 個終端在線",
};

const EN: Strings = Strings {
    open: "Open Manager",
    pause: "Pause Connections",
    exit: "Exit",
    off: "Recognition service is off",
    engine_starting: "Engine starting",
    loading: "Loading models",
    load_failed: "Model loading failed — open the manager for details",
    paused: "Connections paused",
    running: "Running · {n} terminal(s) online",
};

/// 系统界面语言 ⇒ 菜单文字。中文按地区分简繁（台港澳 ⇒ 繁體），其余英文。
fn strings() -> &'static Strings {
    let lang = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    pick(lang)
}

fn pick(langid: u16) -> &'static Strings {
    const LANG_CHINESE: u16 = 0x04;
    if langid & 0x3ff != LANG_CHINESE {
        return &EN;
    }
    match langid {
        0x0404 | 0x0c04 | 0x1404 | 0x7c04 => &ZH_HANT,
        _ => &ZH_HANS,
    }
}

/// 图标提示文字（纯函数，便于单测）。
fn tooltip(s: &Strings, shared: Option<&Shared>) -> String {
    let text = match shared {
        None => s.off.to_string(),
        Some(sh) => {
            let st = sh.status();
            if !st.engine_online {
                s.engine_starting.to_string()
            } else if !st.load_error.is_empty() {
                s.load_failed.to_string()
            } else if !st.local_realtime_ready {
                s.loading.to_string()
            } else if st.paused {
                s.paused.to_string()
            } else {
                s.running.replace("{n}", &st.online.len().to_string())
            }
        }
    };
    format!("flashasr · {text}")
}

pub fn start() {
    if let Err(e) = std::thread::Builder::new().name("tray".into()).spawn(run) {
        log::warn!("[flashasr] 托盘线程启动失败：{e}");
    }
}

/// 退出 / 自重启前调用：让托盘线程收掉图标（`process::exit` 不跑析构，不收会在任务栏留幽灵图标）。最多等 1 秒。
pub fn shutdown() {
    let id = THREAD_ID.load(Ordering::Acquire);
    if id == 0 {
        return;
    }
    unsafe {
        let _ = PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0));
    }
    let deadline = Instant::now() + Duration::from_secs(1);
    while !DONE.load(Ordering::Acquire) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn run() {
    let s = strings();
    let open = MenuItem::new(s.open, true, None);
    let pause = CheckMenuItem::new(s.pause, SHARED.get().is_some(), false, None);
    let exit = MenuItem::new(s.exit, true, None);
    let menu = Menu::new();
    if let Err(e) = menu.append_items(&[&open, &pause, &PredefinedMenuItem::separator(), &exit]) {
        log::warn!("[flashasr] 托盘菜单创建失败：{e}");
        return;
    }
    let mut b = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip(tooltip(s, SHARED.get().map(|a| a.as_ref())));
    match Icon::from_resource(ICON_RESOURCE_ID, None) {
        Ok(i) => b = b.with_icon(i),
        Err(e) => log::warn!("[flashasr] 托盘图标读取失败：{e}"),
    }
    let tray = match b.build() {
        Ok(t) => t,
        Err(e) => {
            log::warn!("[flashasr] 托盘创建失败：{e}");
            return;
        }
    };
    THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::Release);
    unsafe {
        SetTimer(HWND::default(), 0, REFRESH_MS, None);
    }
    let mut msg = MSG::default();
    loop {
        if unsafe { GetMessageW(&mut msg, HWND::default(), 0, 0) }.0 <= 0 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == *open.id() {
                open_ui();
            } else if ev.id == *pause.id() {
                if let Some(sh) = SHARED.get() {
                    sh.set_paused(pause.is_checked());
                }
                refresh(&tray, &pause, s);
            } else if ev.id == *exit.id() {
                log::info!("[flashasr] 托盘「退出」");
                drop(tray);
                std::process::exit(0);
            }
        }
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = ev
            {
                open_ui();
            }
        }
        if msg.message == WM_TIMER {
            refresh(&tray, &pause, s);
        }
    }
    drop(tray);
    DONE.store(true, Ordering::Release);
}

/// 刷新提示文字与「暂停」勾选（暂停也可能是管理界面经控制通道改的）。
fn refresh(tray: &TrayIcon, pause: &CheckMenuItem, s: &Strings) {
    let shared = SHARED.get().map(|a| a.as_ref());
    let _ = tray.set_tooltip(Some(tooltip(s, shared)));
    pause.set_enabled(shared.is_some());
    pause.set_checked(shared.is_some_and(|sh| sh.status().paused));
}

fn open_ui() {
    let Some(exe) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(UI_EXE)))
    else {
        return;
    };
    if let Err(e) = std::process::Command::new(&exe).spawn() {
        log::warn!("[flashasr] 打开管理界面失败（{}）：{e}", exe.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_pick() {
        assert_eq!(pick(0x0804).open, ZH_HANS.open, "简体中文（中国）");
        assert_eq!(pick(0x1004).open, ZH_HANS.open, "新加坡");
        assert_eq!(pick(0x0404).open, ZH_HANT.open, "台湾");
        assert_eq!(pick(0x0c04).open, ZH_HANT.open, "香港");
        assert_eq!(pick(0x0409).open, EN.open, "英语");
        assert_eq!(pick(0x0411).open, EN.open, "日语 ⇒ 英文");
    }

    #[test]
    fn tooltip_when_service_off() {
        assert_eq!(tooltip(&ZH_HANS, None), "flashasr · 识别服务已关闭");
    }
}
