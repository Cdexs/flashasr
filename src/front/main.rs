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
            let relaunch = std::env::current_exe().and_then(|exe| {
                let mut cmd = std::process::Command::new(exe);
                cmd.args(std::env::args().skip(1).filter(|a| a != RELAUNCH_ARG))
                    .arg(RELAUNCH_ARG);
                cmd.spawn()
            });
            if let Err(e) = relaunch {
                log::error!("[flashasr] 自重启失败：{e}；保持当前状态");
                continue;
            }
            #[cfg(target_os = "windows")]
            tray::shutdown();
            std::process::exit(0);
        });
    if let Err(e) = spawn {
        log::warn!("[flashasr] 设置监视线程启动失败：{e}");
    }
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
        log::info!("[flashasr] 总开关关着：不拉起引擎、不监听，等开关打开");
        loop {
            std::thread::park();
        }
    }
    if let Err(e) = run(settings) {
        log::error!("[flashasr] 启动失败：{e}");
        std::process::exit(2);
    }
}

fn run(settings: ServiceSettings) -> std::io::Result<()> {
    let (tls, fp) = tls::load_or_create()?;
    log::info!("[flashasr] 证书指纹 {fp}");
    let link = core_link::CoreLink::start()?;
    let shared = Arc::new(ws::Shared::new(link, settings.port, fp));
    shared.prune();
    // FLASH-P2-06：本机输入法自动发现用的配对文件。
    local_pair::ensure(&shared, settings.port);
    ctl::start(shared.clone())?;
    #[cfg(target_os = "windows")]
    let _ = tray::SHARED.set(shared.clone());

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(serve(shared, TlsAcceptor::from(tls), settings.port))
}

/// 监听 `[::]` 与 `0.0.0.0`。双栈系统（macOS / Linux 默认）上 `[::]` 已覆盖 IPv4，第二个绑定会失败——忽略即可；
/// Windows 的 IPv6 套接字默认只收 IPv6，两个都绑得上。
async fn serve(shared: Arc<ws::Shared>, acceptor: TlsAcceptor, port: u16) -> std::io::Result<()> {
    let mut listeners = Vec::new();
    for addr in [
        SocketAddr::from((Ipv6Addr::UNSPECIFIED, port)),
        SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)),
    ] {
        match TcpListener::bind(addr).await {
            Ok(l) => {
                log::info!("[asr-gateway] 监听 {addr}");
                listeners.push(l);
            }
            Err(e) => log::info!("[asr-gateway] 未监听 {addr}：{e}"),
        }
    }
    if listeners.is_empty() {
        return Err(std::io::Error::other(format!("port {port} unavailable")));
    }
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
