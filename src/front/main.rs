//! flashasr 前台（FLASH-P2-01，原 ASR-SERVER-491 S4 网关）。
//!
//! 常驻、单实例。只做对外的事：TLS、终端鉴权与配对、`run-task` 协议转换、本机管理通道；
//! **不加载任何模型**。识别在引擎 `flashasr-engine` 里 —— 由本进程拉起并看护（`core_link`），
//! 引擎崩了本进程不受影响，几秒后自动重拉。
//!
//! 命令行：`flashasr [-debug]` 常驻运行；`flashasr --add-terminal` 向运行中的实例要一个新终端并打印连接码。

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[allow(dead_code)]
#[path = "../asr_shared/mod.rs"]
mod asr_shared;
#[allow(dead_code)]
#[path = "../asr_service/frame.rs"]
mod frame;
// 内核端 IPC 传输（网关只用其中的本机控制通道 `CtlListener`）。
#[allow(dead_code)]
#[path = "../asr_service/transport.rs"]
mod transport;

mod core_link;
mod ctl;
mod local_pair;
mod status;
mod tls;
#[cfg(target_os = "windows")]
mod tray;
mod ws;

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio_rustls::TlsAcceptor;

use asr_shared::settings::ServiceSettings;

/// 同时在线连接上限（每路一个 TLS 会话 + 少量缓冲；推理通道上限由内核另管）。
const MAX_CONNS: usize = 64;

/// 清除文件句柄的可继承标志（Windows）：防止拉起子进程（引擎 / 自重启 / 管理界面）时把日志文件或
/// `flashasr.lock` 句柄带过去，导致旧实例退出后锁仍被子进程占住。
#[cfg(target_os = "windows")]
fn mark_non_inheritable_file(file: &std::fs::File) {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{
        SetHandleInformation, HANDLE, HANDLE_FLAGS, HANDLE_FLAG_INHERIT,
    };
    // SAFETY：`file` 持有有效内核句柄，清除 `HANDLE_FLAG_INHERIT` 不影响本进程读写与文件锁。
    unsafe {
        let _ = SetHandleInformation(
            HANDLE(file.as_raw_handle() as _),
            HANDLE_FLAG_INHERIT.0,
            HANDLE_FLAGS(0),
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn mark_non_inheritable_file(_file: &std::fs::File) {}

/// 清除套接字的可继承标志（Windows）：Winsock 套接字默认带 `HANDLE_FLAG_INHERIT`，而 `Command::spawn`
/// 拉起引擎（需管道重定向 ⇒ `bInheritHandles = TRUE`）时会把所有可继承句柄复制进 `flashasr-engine`；
/// 若前台自重启时引擎尚未完全退出，旧 7860 监听套接字仍被引擎持有，新实例绑定即报 `os error 10048`。
#[cfg(target_os = "windows")]
fn mark_non_inheritable_listener(listener: &TcpListener) {
    use std::os::windows::io::AsRawSocket;
    use windows::Win32::Foundation::{
        SetHandleInformation, HANDLE, HANDLE_FLAGS, HANDLE_FLAG_INHERIT,
    };
    // SAFETY：`TcpListener` 底层 SOCKET 在 Win32 下是内核对象句柄，支持 `SetHandleInformation`。
    unsafe {
        let _ = SetHandleInformation(
            HANDLE(listener.as_raw_socket() as _),
            HANDLE_FLAG_INHERIT.0,
            HANDLE_FLAGS(0),
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn mark_non_inheritable_listener(_listener: &TcpListener) {}

/// 日志写到程序旁边的 `flashasr.log`；`-debug` ⇒ Debug 级。
fn init_logging(debug: bool) {
    let level = if debug {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    let mut b = env_logger::Builder::new();
    b.filter_level(level)
        .format_timestamp(Some(env_logger::TimestampPrecision::Millis));
    if let Ok(f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(asr_shared::data_dir().join("flashasr.log"))
    {
        mark_non_inheritable_file(&f);
        b.target(env_logger::Target::Pipe(Box::new(f)));
    }
    b.init();
}

/// `--add-terminal`：向正在运行的实例要一个新终端，把连接码打到标准输出（管理界面之前的无界面配对）。
/// 连接码含一次性明文 key，只打一次、不落盘。服务没在运行 ⇒ 退出码 2。
fn add_terminal_cli() -> i32 {
    use asr_shared::control::{self, Request};
    match control::call(&Request::AddTerminal) {
        Ok(r) if r.error.is_empty() => match r.added {
            Some(a) => {
                println!("{}", a.uri);
                0
            }
            None => {
                eprintln!("flashasr returned no terminal");
                1
            }
        },
        Ok(r) => {
            eprintln!("add terminal failed: {}", r.error);
            1
        }
        Err(e) => {
            eprintln!("flashasr not running: {e}");
            2
        }
    }
}

/// 单实例：独占锁 `<数据目录>/flashasr.lock`（进程退出即释放）。`wait` ⇒ 最多等 10 秒（自重启时旧实例正在退出）。
fn instance_lock(wait: bool) -> Option<std::fs::File> {
    let path = asr_shared::data_dir().join("flashasr.lock");
    let deadline = std::time::Instant::now() + Duration::from_secs(if wait { 10 } else { 0 });
    loop {
        if let Ok(f) = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
        {
            mark_non_inheritable_file(&f);
            if f.try_lock().is_ok() {
                return Some(f);
            }
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// flashasr 本身就是识别服务 ⇒ 总开关缺省开（首次运行写入设置文件）；用户在设置文件里关掉则尊重。
fn ensure_service_enabled_by_default() {
    if ServiceSettings::path().exists() {
        return;
    }
    let mut s = ServiceSettings::load();
    s.enabled = true;
    match s.save() {
        Ok(()) => log::info!("[flashasr] 首次运行：总开关缺省开"),
        Err(e) => log::warn!("[flashasr] 写默认设置失败：{e}"),
    }
}

/// 设置里的总开关 / 端口改了 ⇒ 本进程重启自己（新实例等旧实例退出后接锁；引擎随旧实例的标准输入断开而退出）。
/// 端口要重新监听、配对信息里的地址要换，整体重来最不易出错；这类改动很少发生。
fn watch_settings(start: (bool, u16)) {
    let spawn = std::thread::Builder::new()
        .name("settings-watch".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            let s = ServiceSettings::load();
            if (s.enabled, s.port) == start {
                continue;
            }
            log::info!(
                "[flashasr] 设置已改（开关 {} ⇒ {}，端口 {} ⇒ {}），重启服务",
                start.0,
                s.enabled,
                start.1,
                s.port
            );
            let relaunch = std::env::current_exe().and_then(|exe| spawn_relaunch(&exe));
            if let Err(e) = relaunch {
                log::error!("[flashasr] 自重启失败：{e}；保持当前状态");
                continue;
            }
            asr_shared::control::GatewayInfo::remove();
            #[cfg(target_os = "windows")]
            tray::shutdown();
            std::process::exit(0);
        });
    if let Err(e) = spawn {
        log::warn!("[flashasr] 设置监视线程启动失败：{e}");
    }
}

/// 拉起自重启新实例（不继承当前实例任何句柄）。
#[cfg(target_os = "windows")]
fn spawn_relaunch(exe: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        CreateProcessW, DETACHED_PROCESS, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION, STARTUPINFOW,
    };

    let app: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut parts = vec![format!("\"{}\"", exe.display())];
    for a in std::env::args().skip(1).filter(|a| a != RELAUNCH_ARG) {
        parts.push(format!("\"{}\"", a.replace('"', "\\\"")));
    }
    parts.push(RELAUNCH_ARG.to_string());
    let mut cmdline: Vec<u16> = parts.join(" ").encode_utf16().chain(Some(0)).collect();
    let cwd: Option<Vec<u16>> = exe
        .parent()
        .map(|d| d.as_os_str().encode_wide().chain(Some(0)).collect());
    let si = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut pi = PROCESS_INFORMATION::default();
    // SAFETY：缓冲区在调用期间有效，bInheritHandles = FALSE 保证新实例不继承旧套接字或文件锁。
    unsafe {
        CreateProcessW(
            PCWSTR(app.as_ptr()),
            PWSTR(cmdline.as_mut_ptr()),
            None,
            None,
            false,
            PROCESS_CREATION_FLAGS(DETACHED_PROCESS.0),
            None,
            cwd.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
            &si,
            &mut pi,
        )
    }
    .map_err(|e| std::io::Error::from_raw_os_error(e.code().0 & 0xFFFF))?;
    unsafe {
        let _ = CloseHandle(pi.hThread);
        let _ = CloseHandle(pi.hProcess);
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn spawn_relaunch(exe: &std::path::Path) -> std::io::Result<()> {
    use std::process::Stdio;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(std::env::args().skip(1).filter(|a| a != RELAUNCH_ARG))
        .arg(RELAUNCH_ARG)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.spawn().map(|_| ())
}

/// 自重启时新实例带的参数（等旧实例释放单实例锁）。
const RELAUNCH_ARG: &str = "--relaunch";

fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    if std::env::args().any(|a| a == "--add-terminal") {
        std::process::exit(add_terminal_cli());
    }
    let debug = std::env::args().any(|a| a == "-debug" || a == "--debug");
    init_logging(debug);
    let relaunch = std::env::args().any(|a| a == RELAUNCH_ARG);
    let Some(_lock) = instance_lock(relaunch) else {
        log::info!("[flashasr] 已有实例在运行，本次退出");
        return;
    };
    log::info!("flashasr {} starting", env!("CARGO_PKG_VERSION"));
    ensure_service_enabled_by_default();
    let settings = ServiceSettings::load();
    watch_settings((settings.enabled, settings.port));
    // FLASH-P2b-02：托盘在总开关关着时也要在（否则用户没有入口打开管理界面）；服务起来后再挂上状态。
    #[cfg(target_os = "windows")]
    tray::start();
    if !settings.enabled {
        asr_shared::control::GatewayInfo::remove();
        log::info!("[flashasr] 总开关关着：不拉起引擎、不监听，等开关打开");
        loop {
            std::thread::park();
        }
    }
    if let Err(e) = run(settings, relaunch) {
        asr_shared::control::GatewayInfo::remove();
        log::error!("[flashasr] 启动失败：{e}；保持常驻等待设置变更或重试");
        loop {
            std::thread::park();
        }
    }
}

/// 绑定端口重试参数：自重启或快速「停止 → 启动」时，旧进程刚退出、内核释放监听端口可能需要短暂时间。
const BIND_RETRY_ATTEMPTS: usize = 25;
const BIND_RETRY_DELAY: Duration = Duration::from_millis(200);

fn run(settings: ServiceSettings, retry_bind: bool) -> std::io::Result<()> {
    let (tls, fp) = tls::load_or_create()?;
    log::info!("[flashasr] 证书指纹 {fp}");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    // 先绑定端口，确认端口可用后再拉起引擎（避免端口冲突时白白拉起引擎又瞬间断开杀死它）。
    let listeners = rt.block_on(bind_listeners(settings.port, retry_bind))?;
    let link = core_link::CoreLink::start()?;
    let shared = Arc::new(ws::Shared::new(link, settings.port, fp));
    shared.prune();
    // FLASH-P2-06：本机输入法自动发现用的配对文件。
    local_pair::ensure(&shared, settings.port);
    ctl::start(shared.clone())?;
    #[cfg(target_os = "windows")]
    let _ = tray::SHARED.set(shared.clone());

    rt.block_on(serve(shared, TlsAcceptor::from(tls), listeners))
}

/// 监听 `[::]` 与 `0.0.0.0`。双栈系统（macOS / Linux 默认）上 `[::]` 已覆盖 IPv4，第二个绑定会失败——忽略即可；
/// Windows 的 IPv6 套接字默认只收 IPv6，两个都绑得上。
async fn bind_listeners(port: u16, retry_on_busy: bool) -> std::io::Result<Vec<TcpListener>> {
    let max_tries = if retry_on_busy {
        BIND_RETRY_ATTEMPTS
    } else {
        5
    };
    for attempt in 1..=max_tries {
        let mut listeners = Vec::new();
        let mut any_addr_in_use = false;
        for addr in [
            SocketAddr::from((Ipv6Addr::UNSPECIFIED, port)),
            SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)),
        ] {
            match TcpListener::bind(addr).await {
                Ok(l) => {
                    mark_non_inheritable_listener(&l);
                    log::info!("[asr-gateway] 监听 {addr}");
                    listeners.push(l);
                }
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::AddrInUse {
                        any_addr_in_use = true;
                    }
                    if attempt == max_tries {
                        log::info!("[asr-gateway] 未监听 {addr}：{e}");
                    }
                }
            }
        }
        if !listeners.is_empty() {
            return Ok(listeners);
        }
        if !any_addr_in_use || attempt == max_tries {
            break;
        }
        tokio::time::sleep(BIND_RETRY_DELAY).await;
    }
    Err(std::io::Error::other(format!("port {port} unavailable")))
}

async fn serve(
    shared: Arc<ws::Shared>,
    acceptor: TlsAcceptor,
    listeners: Vec<TcpListener>,
) -> std::io::Result<()> {
    let limit = Arc::new(Semaphore::new(MAX_CONNS));
    let mut tasks = Vec::new();
    for l in listeners {
        let (shared, acceptor, limit) = (shared.clone(), acceptor.clone(), limit.clone());
        tasks.push(tokio::spawn(async move {
            loop {
                let (tcp, peer) = match l.accept().await {
                    Ok(x) => x,
                    Err(_) => {
                        // 句柄耗尽之类：稍等再接，避免空转吃满 CPU。
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        continue;
                    }
                };
                // 满了直接断：不排队，客户端按网络错误重试。
                let Ok(permit) = limit.clone().try_acquire_owned() else {
                    continue;
                };
                let (shared, acceptor) = (shared.clone(), acceptor.clone());
                tokio::spawn(async move {
                    ws::serve_conn(shared, acceptor, tcp, peer).await;
                    drop(permit);
                });
            }
        }));
    }
    for t in tasks {
        let _ = t.await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_listeners_retries_when_port_briefly_busy() {
        let v4_holder = std::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        let port = v4_holder.local_addr().unwrap().port();
        let v6_holder = std::net::TcpListener::bind((Ipv6Addr::UNSPECIFIED, port)).ok();

        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            drop(v4_holder);
            drop(v6_holder);
        });

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let res = rt.block_on(bind_listeners(port, true));
        assert!(res.is_ok(), "bind_listeners 应重试成功: {:?}", res);
        let listeners = res.unwrap();
        assert!(!listeners.is_empty(), "listeners 不应为空");
    }

    #[test]
    fn mark_non_inheritable_file_keeps_lock_working() {
        let tmp_file_path = std::env::temp_dir().join(format!(
            "flashasr_test_lock_{}_{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let f = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&tmp_file_path)
            .unwrap();

        mark_non_inheritable_file(&f);
        assert!(f.try_lock().is_ok(), "mark_non_inheritable 后锁应依然可用");

        drop(f);
        let _ = std::fs::remove_file(&tmp_file_path);
    }
}
