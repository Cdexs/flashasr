//! FLASH-P2-01：前台 ↔ 引擎的连接，前台负责拉起并看护引擎。
//!
//! - 前台拉起 `flashasr-engine`（口令经环境变量传入，不上命令行），从它标准输出第一行读 IPC 端点，再连上去；
//! - 引擎的标准输入由前台握着：前台退出 / 崩溃 ⇒ 管道断 ⇒ 引擎自行退出，不留孤儿进程；
//! - 引擎掉线（崩溃、被杀、IPC 断）⇒ 进行中的会话立刻收到 `engine_restarted`，[`RESPAWN_DELAY`] 后重拉重连；
//!   重连之前新会话直接回 `not_ready`（终端看到「模型未就绪」，稍后重试即可）。
//!
//! 对 `ws.rs` 的接口（`open` / `send` / `close`）与原网关版一致，调用方不感知引擎重启。

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ring::rand::{SecureRandom, SystemRandom};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::frame::{read_frame, write_frame, EndPayload, Frame, Kind, OpenParams};

/// 引擎掉线后多久重拉。
const RESPAWN_DELAY: Duration = Duration::from_secs(5);
/// 引擎程序名（与前台同目录）。
#[cfg(target_os = "windows")]
const ENGINE_EXE: &str = "flashasr-engine.exe";
#[cfg(not(target_os = "windows"))]
const ENGINE_EXE: &str = "flashasr-engine";
/// 与引擎约定（见 `src/engine/main.rs`）：口令环境变量、标准输出第一行的端点前缀。
const TOKEN_ENV: &str = "FLASHASR_TOKEN";
const ENDPOINT_PREFIX: &str = "FLASHASR_ENDPOINT ";
/// FLASH-P2-04：引擎状态行前缀（之后每行一个 JSON，见引擎 `report_status`）。
const STATUS_PREFIX: &str = "FLASHASR_STATUS ";

/// FLASH-P2-04：引擎报上来的状态（引擎不在线 ⇒ 全默认）。
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
pub struct EngineStatus {
    /// 本地实时模型已加载完、可识别。
    #[serde(default)]
    pub local_realtime: bool,
    /// 服务端开了大模型（输出优化 / 大模型翻译可用）。
    #[serde(default)]
    pub llm: bool,
}
/// 引擎掉线时进行中会话的错误码（`ws.rs` `end_code` 映射为 `EngineRestarted`）。
pub const ERR_ENGINE_RESTARTED: &str = "engine_restarted";
/// 引擎不在线时新会话的错误码（同引擎回的「模型未就绪」）。
const ERR_NOT_READY: &str = "not_ready";

type Routes = Arc<Mutex<HashMap<u64, UnboundedSender<Frame>>>>;

struct Inner {
    /// 发往引擎的写队列；`None` = 引擎不在线。
    tx: Mutex<Option<mpsc::Sender<Frame>>>,
    routes: Routes,
    status: Mutex<EngineStatus>,
}

pub struct CoreLink {
    inner: Arc<Inner>,
    next: AtomicU64,
}

impl CoreLink {
    /// 起看护线程：拉起引擎、连接，掉线重拉。立即返回（引擎上线前新会话回 `not_ready`）。
    pub fn start() -> io::Result<Self> {
        let exe = std::env::current_exe()?
            .parent()
            .map(|d| d.join(ENGINE_EXE))
            .ok_or_else(|| io::Error::other("no exe dir"))?;
        let inner = Arc::new(Inner {
            tx: Mutex::new(None),
            routes: Arc::default(),
            status: Mutex::default(),
        });
        let i2 = inner.clone();
        std::thread::Builder::new()
            .name("engine-supervisor".into())
            .spawn(move || loop {
                match run_engine_once(&exe, &i2) {
                    Ok(()) => {
                        log::warn!("[flashasr] 引擎已断开，{}s 后重启", RESPAWN_DELAY.as_secs())
                    }
                    Err(e) => log::warn!(
                        "[flashasr] 引擎启动 / 连接失败：{e}；{}s 后重试",
                        RESPAWN_DELAY.as_secs()
                    ),
                }
                fail_all(&i2, ERR_ENGINE_RESTARTED);
                std::thread::sleep(RESPAWN_DELAY);
            })?;
        Ok(Self {
            inner,
            next: AtomicU64::new(1),
        })
    }

    /// 引擎此刻是否在线（`/v1/status` 用）。
    pub fn engine_up(&self) -> bool {
        lock(&self.inner.tx).is_some()
    }

    /// 引擎最近一次报的状态（不在线 ⇒ 全默认）。
    pub fn engine_status(&self) -> EngineStatus {
        lock(&self.inner.status).clone()
    }

    pub fn open(&self, params: &OpenParams) -> (u64, UnboundedReceiver<Frame>) {
        let sid = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = unbounded_channel();
        lock(&self.inner.routes).insert(sid, tx.clone());
        if !self.try_send(Frame::json(Kind::Open, sid, params)) {
            lock(&self.inner.routes).remove(&sid);
            let _ = tx.send(end_frame(sid, ERR_NOT_READY));
        }
        (sid, rx)
    }

    pub fn send(&self, f: Frame) {
        let _ = self.try_send(f);
    }

    pub fn close(&self, sid: u64) {
        lock(&self.inner.routes).remove(&sid);
    }

    fn try_send(&self, f: Frame) -> bool {
        match lock(&self.inner.tx).as_ref() {
            Some(tx) => tx.send(f).is_ok(),
            None => false,
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn end_frame(sid: u64, error: &str) -> Frame {
    Frame::json(
        Kind::End,
        sid,
        &EndPayload {
            error: error.to_string(),
            ..Default::default()
        },
    )
}

/// 引擎掉线：所有进行中的会话收到错误结束（终端回 `task-failed`），路由清空。
fn fail_all(inner: &Inner, error: &str) {
    let mut routes = lock(&inner.routes);
    for (sid, tx) in routes.drain() {
        let _ = tx.send(end_frame(sid, error));
    }
}

/// 拉起一次引擎并服务到它断开为止。返回 = 引擎已不在（调用方负责清会话、延时重拉）。
fn run_engine_once(exe: &PathBuf, inner: &Arc<Inner>) -> io::Result<()> {
    let token = new_token()?;
    let mut cmd = Command::new(exe);
    for a in std::env::args().skip(1) {
        if a == "-debug" || a == "--debug" {
            cmd.arg(a);
        }
    }
    cmd.env(TOKEN_ENV, &token)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn()?;
    log::info!("[flashasr] 引擎已拉起（pid {}）", child.id());
    let r = serve_engine(&mut child, &token, inner);
    // 不在线了：先撤写队列、清状态，再结束子进程（它若还活着，标准输入随 Child 一起关闭也会让它退出）。
    *lock(&inner.tx) = None;
    *lock(&inner.status) = EngineStatus::default();
    let _ = child.kill();
    let _ = child.wait();
    r
}

fn serve_engine(child: &mut Child, token: &str, inner: &Arc<Inner>) -> io::Result<()> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("engine stdout missing"))?;
    let mut lines = BufReader::new(stdout);
    let mut first = String::new();
    lines.read_line(&mut first)?;
    let endpoint = first
        .trim()
        .strip_prefix(ENDPOINT_PREFIX)
        .ok_or_else(|| io::Error::other(format!("unexpected engine output: {first:?}")))?
        .to_string();
    let (reader, mut writer) = open_with_retry(&endpoint, child)?;
    write_frame(
        &mut writer,
        &Frame::new(Kind::Hello, 0, token.as_bytes().to_vec()),
    )?;
    let (tx, rx) = mpsc::channel::<Frame>();
    std::thread::Builder::new()
        .name("engine-write".into())
        .spawn(move || {
            while let Ok(f) = rx.recv() {
                if let Err(e) = write_frame(&mut writer, &f) {
                    log::warn!("[flashasr] 写引擎失败：{e}");
                    break;
                }
            }
        })?;
    *lock(&inner.tx) = Some(tx);
    log::info!("[flashasr] 已连上引擎（{endpoint}）");
    // FLASH-P2-04：之后的标准输出是状态行；读到引擎退出（EOF）为止。
    let i2 = inner.clone();
    std::thread::Builder::new()
        .name("engine-status".into())
        .spawn(move || {
            for line in lines.lines().map_while(Result::ok) {
                if let Some(st) = parse_status(&line) {
                    *lock(&i2.status) = st;
                }
            }
        })?;
    read_loop(reader, &inner.routes)
}

fn parse_status(line: &str) -> Option<EngineStatus> {
    serde_json::from_str(line.trim().strip_prefix(STATUS_PREFIX)?).ok()
}

fn read_loop(mut reader: Box<dyn Read + Send>, routes: &Routes) -> io::Result<()> {
    loop {
        match read_frame(&mut reader)? {
            Some(f) => {
                let end = f.kind == Kind::End;
                let mut map = lock(routes);
                if let Some(tx) = map.get(&f.session) {
                    let _ = tx.send(f.clone());
                }
                if end {
                    map.remove(&f.session);
                }
            }
            None => return Ok(()),
        }
    }
}

fn new_token() -> io::Result<String> {
    let mut b = [0u8; 32];
    SystemRandom::new()
        .fill(&mut b)
        .map_err(|_| io::Error::other("system random unavailable"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

type Pair = (Box<dyn Read + Send>, Box<dyn Write + Send>);

/// 连引擎 IPC 的最长等待（引擎报出端点时管道还没建：管道在其服务线程 `accept` 里才创建）。
const OPEN_TIMEOUT: Duration = Duration::from_secs(10);
const OPEN_RETRY: Duration = Duration::from_millis(50);

/// FLASH-P2-01：引擎先报端点、后在服务线程里建管道 ⇒ 刚报出时连不上（找不到文件）。重试到连上为止；
/// 引擎中途退出 / 超时 ⇒ 报错（调用方延时重拉）。
fn open_with_retry(endpoint: &str, child: &mut Child) -> io::Result<Pair> {
    let deadline = std::time::Instant::now() + OPEN_TIMEOUT;
    loop {
        match open(endpoint) {
            Ok(p) => return Ok(p),
            Err(e) => {
                if let Ok(Some(st)) = child.try_wait() {
                    return Err(io::Error::other(format!(
                        "engine exited before IPC was ready: {st}"
                    )));
                }
                if std::time::Instant::now() >= deadline {
                    return Err(io::Error::new(
                        e.kind(),
                        format!("engine IPC not ready in {}s: {e}", OPEN_TIMEOUT.as_secs()),
                    ));
                }
                std::thread::sleep(OPEN_RETRY);
            }
        }
    }
}

/// 打开顺序与引擎 `accept` 的等待顺序一致。
#[cfg(target_os = "windows")]
fn open(endpoint: &str) -> io::Result<Pair> {
    use std::fs::OpenOptions;
    let w = OpenOptions::new()
        .write(true)
        .open(format!("{endpoint}.in"))?;
    // `.in` 已连上（引擎在等 Hello）⇒ `.out` 必然紧随其后建好；只在这里短等，不让外层重试重开 `.in`
    //（重开会打断引擎这边已开始的握手）。
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        match OpenOptions::new()
            .read(true)
            .open(format!("{endpoint}.out"))
        {
            Ok(r) => return Ok((Box::new(r), Box::new(w))),
            Err(e) if std::time::Instant::now() >= deadline => return Err(e),
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

#[cfg(unix)]
fn open(endpoint: &str) -> io::Result<Pair> {
    let s = std::os::unix::net::UnixStream::connect(endpoint)?;
    let r = s.try_clone()?;
    Ok((Box::new(r), Box::new(s)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 引擎不在线 ⇒ 新会话立刻收到 `not_ready` 结束（不挂起）；路由不残留。
    #[test]
    fn open_while_engine_down_ends_not_ready() {
        let link = CoreLink {
            inner: Arc::new(Inner {
                tx: Mutex::new(None),
                routes: Arc::default(),
                status: Mutex::default(),
            }),
            next: AtomicU64::new(1),
        };
        assert!(!link.engine_up());
        let (sid, mut rx) = link.open(&OpenParams::default());
        let f = rx.try_recv().expect("立即收到结束帧");
        assert_eq!(f.kind, Kind::End);
        assert_eq!(f.session, sid);
        let end: EndPayload = serde_json::from_slice(&f.body).unwrap();
        assert_eq!(end.error, ERR_NOT_READY);
        assert!(lock(&link.inner.routes).is_empty());
    }

    /// 引擎掉线 ⇒ 每个进行中的会话收到 `engine_restarted`，路由清空。
    #[test]
    fn fail_all_ends_every_session() {
        let inner = Inner {
            tx: Mutex::new(None),
            routes: Arc::default(),
            status: Mutex::default(),
        };
        let mut rxs = Vec::new();
        for sid in 1..=3u64 {
            let (tx, rx) = unbounded_channel();
            lock(&inner.routes).insert(sid, tx);
            rxs.push((sid, rx));
        }
        fail_all(&inner, ERR_ENGINE_RESTARTED);
        for (sid, mut rx) in rxs {
            let f = rx.try_recv().expect("收到结束帧");
            assert_eq!((f.kind, f.session), (Kind::End, sid));
            let end: EndPayload = serde_json::from_slice(&f.body).unwrap();
            assert_eq!(end.error, ERR_ENGINE_RESTARTED);
        }
        assert!(lock(&inner.routes).is_empty());
    }

    /// FLASH-P2-04：状态行解析；不是状态行 / JSON 坏 ⇒ `None`（不覆盖已有状态）。
    #[test]
    fn status_line_parsing() {
        assert_eq!(
            parse_status(r#"FLASHASR_STATUS {"local_realtime":true,"llm":false}"#),
            Some(EngineStatus {
                local_realtime: true,
                llm: false
            })
        );
        assert_eq!(
            parse_status("FLASHASR_STATUS {}"),
            Some(EngineStatus::default())
        );
        assert_eq!(parse_status("hello"), None);
        assert_eq!(parse_status("FLASHASR_STATUS {bad"), None);
    }
}
