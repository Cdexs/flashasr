//! FLASH-P2-04：`GET /v1/status`（与 WebSocket 同一 TLS 端口）。
//!
//! 做法：TLS 握手后先读完请求头。是 `GET /v1/status` ⇒ 验终端 key 后回 JSON、断开；否则把已读字节
//! 原样「倒回」（[`Rewind`]）交给 WebSocket 握手 —— 对 WebSocket 终端完全透明。
//!
//! 返回内容（终端据此决定能不能发识别、要不要提示升级 / 稍后再试）：
//! `{"service":"flashasr","version":"0.1.0","protocol":1,"engine_online":true,"local_realtime_ready":true,
//!   "llm_configured":false,"paused":false}`

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf};

/// 对外协议版本（`docs/ASR-PROTOCOL.md` v1）。
pub const PROTOCOL_VERSION: u32 = 1;
/// 请求头最长字节数（WebSocket 升级头远小于此）。
const MAX_HEAD: usize = 16 * 1024;

/// 已读出的字节先吐回去，再接着读底层流；写直通。
pub struct Rewind<S> {
    pre: Vec<u8>,
    pos: usize,
    inner: S,
}

impl<S> Rewind<S> {
    pub fn new(pre: Vec<u8>, inner: S) -> Self {
        Self { pre, pos: 0, inner }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for Rewind<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.pos < self.pre.len() {
            let n = (self.pre.len() - self.pos).min(buf.remaining());
            let (pos, end) = (self.pos, self.pos + n);
            buf.put_slice(&self.pre[pos..end]);
            self.pos = end;
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Rewind<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// 读到请求头结束（`\r\n\r\n`）为止；多读进来的字节一并返回（倒回时一起吐出）。
/// 连接关闭 / 超长 ⇒ 已读部分照返回（交给 WebSocket 握手去判失败）。
pub async fn read_head<S: AsyncRead + Unpin>(s: &mut S) -> io::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < MAX_HEAD {
        let n = s.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(buf)
}

/// 普通 HTTP 接口（非 WebSocket）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Api {
    /// `GET /v1/status`
    Status,
    /// `POST /v1/profile/voiceprint/reset`（FLASH-P2-05：重置调用方终端自己的声纹）
    VoiceprintReset,
}

/// 请求是本模块的 HTTP 接口 ⇒ `Some((接口, Authorization 里的 bearer key))`（没带 key ⇒ 第二元 `None`）；
/// 不是 ⇒ `None`（按 WebSocket 处理）。查询串忽略。
pub fn api_request(head: &[u8]) -> Option<(Api, Option<String>)> {
    let text = std::str::from_utf8(head).ok()?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split_whitespace();
    let (method, target) = (first.next()?, first.next()?);
    let path = target.split('?').next().unwrap_or(target);
    let api = match (method.to_ascii_uppercase().as_str(), path) {
        ("GET", "/v1/status") => Api::Status,
        ("POST", "/v1/profile/voiceprint/reset") => Api::VoiceprintReset,
        _ => return None,
    };
    let key = lines
        .take_while(|l| !l.is_empty())
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("authorization"))
        .and_then(|(_, v)| {
            let (scheme, key) = v.trim().split_once(' ')?;
            scheme
                .eq_ignore_ascii_case("bearer")
                .then(|| key.trim().to_string())
        });
    Some((api, key))
}

/// 状态内容（纯函数，便于单测）。
pub fn status_body(
    engine_online: bool,
    local_realtime_ready: bool,
    llm: bool,
    paused: bool,
) -> Value {
    json!({
        "service": "flashasr",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": PROTOCOL_VERSION,
        "engine_online": engine_online,
        "local_realtime_ready": engine_online && local_realtime_ready,
        "llm_configured": engine_online && llm,
        "paused": paused,
    })
}

/// 组一个完整的 HTTP/1.1 响应（`Connection: close`）。
pub fn http_response(code: u16, reason: &str, body: &Value) -> Vec<u8> {
    let body = body.to_string();
    format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_api_requests_and_key() {
        let h = b"GET /v1/status HTTP/1.1\r\nHost: a\r\nAuthorization: Bearer ABC-123\r\n\r\n";
        assert_eq!(
            api_request(h),
            Some((Api::Status, Some("ABC-123".to_string())))
        );
        let h = b"GET /v1/status?x=1 HTTP/1.1\r\nHost: a\r\n\r\n";
        assert_eq!(api_request(h), Some((Api::Status, None)), "没带 key");
        let h = b"POST /v1/profile/voiceprint/reset HTTP/1.1\r\nAuthorization: bearer K\r\n\r\n";
        assert_eq!(
            api_request(h),
            Some((Api::VoiceprintReset, Some("K".to_string())))
        );
        let h = b"GET /api-ws/v1/inference HTTP/1.1\r\nUpgrade: websocket\r\n\r\n";
        assert_eq!(api_request(h), None, "WebSocket 升级不归这里");
        assert_eq!(api_request(b"POST /v1/status HTTP/1.1\r\n\r\n"), None);
        assert_eq!(
            api_request(b"GET /v1/profile/voiceprint/reset HTTP/1.1\r\n\r\n"),
            None
        );
        assert_eq!(api_request(b"\xff\xfe"), None);
    }

    #[test]
    fn body_hides_engine_details_when_offline() {
        let b = status_body(false, true, true, false);
        assert_eq!(b["engine_online"], false);
        assert_eq!(b["local_realtime_ready"], false, "引擎不在线时不报就绪");
        assert_eq!(b["llm_configured"], false);
        let b = status_body(true, true, false, true);
        assert_eq!(b["local_realtime_ready"], true);
        assert_eq!(b["paused"], true);
        assert_eq!(b["protocol"], PROTOCOL_VERSION);
    }

    #[test]
    fn http_response_has_length() {
        let r = String::from_utf8(http_response(200, "OK", &json!({"a": 1}))).unwrap();
        assert!(r.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(r.contains("Content-Length: 7\r\n"));
        assert!(r.ends_with("\r\n\r\n{\"a\":1}"));
    }

    /// 倒回：先吐出已读字节，再接着读底层流 —— WebSocket 握手看到的是完整原始字节。
    #[tokio::test]
    async fn rewind_replays_then_continues() {
        let inner: &[u8] = b" world";
        let mut r = Rewind::new(b"hello".to_vec(), inner);
        let mut out = String::new();
        r.read_to_string(&mut out).await.unwrap();
        assert_eq!(out, "hello world");
    }
}
