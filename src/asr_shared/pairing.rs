//! ASR-SERVER-491：终端配对（`asr-terminals.json`，设计 §8.2 / §8.4 / §16.3）。
//!
//! - 每台终端一把 key；文件里只存 key 的 SHA-256，明文只在「添加终端」那一刻给出（二维码 / 连接码）。
//! - **首次使用即绑定**：key 生成后 10 分钟内须被某台终端用上，用上即与该终端的设备代码绑定；
//!   之后设备代码不一致 ⇒ 拒绝（key 被抄走、换台设备也用不了）。
//! - 撤销：立即作废（网关同时断开该终端）；记录保留，列表里显示「已撤销」，统计不丢。

use ring::digest;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 新 key 等待首次绑定的时限（秒）。
pub const BIND_WINDOW_SECS: i64 = 600;
/// 用户 ID 最大长度（1~12 位大小写字母+数字，且必须以字母开头）。
pub const MAX_USER_ID_LEN: usize = 12;
/// 设备码最大长度（1~32 位，由前端根据 CPU+主板序列号生成）。
pub const MAX_DEVICE_ID_LEN: usize = 32;
/// 本机自动配对终端未显式传 user_id 时的缺省用户 ID。
pub const DEFAULT_LOCAL_USER_ID: &str = "local";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Terminal {
    /// 内部编号（统计库的 `terminal_id`）。
    pub id: String,
    /// key 的 SHA-256（十六进制）。
    pub key_hash: String,
    /// 6 位数字配对码的 SHA-256（十六进制；本机终端为空）。明文只在「添加终端」那一刻与二维码/连接码一道给出。
    pub pin_hash: String,
    /// 绑定的用户 ID（最长 12 位的大小写字母+数字，且字母开头；空 = 尚未绑定用户）。
    pub user_id: String,
    /// 绑定的设备代码（空 = 尚未绑定）。
    pub device_id: String,
    /// 终端名称（对方传来；每次连接按最新值更新）。
    pub device_name: String,
    pub created_ts: i64,
    pub last_login_ts: i64,
    pub last_logout_ts: i64,
    /// 最近一次接入方式：`lan` / `wan`。
    pub last_net: String,
    pub revoked: bool,
    /// FLASH-P2-06：本机终端（flashasr 首次运行自动建，供本机输入法自动发现）：不受绑定时限约束
    ///（输入法可能隔很久才第一次连），其 key 只接受本机回环来源（网关握手时判，见 `ws.rs`）。
    pub local: bool,
}

impl Terminal {
    pub fn bound(&self) -> bool {
        !self.device_id.is_empty()
    }

    /// 该终端生效的声纹/词库归属键：已绑定 `user_id` 则用 `user_id`，否则回退终端内部编号 `id`。
    pub fn voiceprint_key(&self) -> &str {
        if !self.user_id.is_empty() {
            &self.user_id
        } else {
            &self.id
        }
    }
}

/// 鉴权失败原因（网关据此回错误码；第三方客户端照文档处理）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    /// key 不存在 / 格式不对。
    InvalidKey,
    /// 未绑定的 key 已过 10 分钟。
    Expired,
    /// 已撤销。
    Revoked,
    /// 首次配对时 6 位数字配对码缺失或错误。
    InvalidPairCode,
    /// 用户 ID 缺失或不符合「字母开头、最长 12 位大小写字母+数字」规则。
    InvalidUserId,
    /// 用户 ID 与该终端已绑定的用户不一致。
    UserMismatch,
    /// 设备代码与绑定的不一致。
    DeviceMismatch,
    /// 缺设备代码 / 名称，或设备代码超过 32 位。
    MissingDevice,
}

impl AuthError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidKey => "InvalidKey",
            Self::Expired => "KeyExpired",
            Self::Revoked => "KeyRevoked",
            Self::InvalidPairCode => "InvalidPairCode",
            Self::InvalidUserId => "InvalidUserId",
            Self::UserMismatch => "UserMismatch",
            Self::DeviceMismatch => "DeviceMismatch",
            Self::MissingDevice => "MissingDeviceInfo",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalStore {
    pub terminals: Vec<Terminal>,
}

fn sha256_hex(s: &str) -> String {
    sha256_hex_bytes(s.as_bytes())
}

fn sha256_hex_bytes(b: &[u8]) -> String {
    digest::digest(&digest::SHA256, b)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// RFC 4648 base32（大写、无填充）。16 字节 ⇒ 26 字符：手输也不至于太长。
pub(super) fn base32(bytes: &[u8]) -> String {
    const A: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let (mut buf, mut bits) = (0u32, 0u32);
    for &b in bytes {
        buf = (buf << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(A[((buf >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(A[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

pub(super) fn random_bytes<const N: usize>() -> std::io::Result<[u8; N]> {
    let mut b = [0u8; N];
    SystemRandom::new()
        .fill(&mut b)
        .map_err(|_| std::io::Error::other("system random unavailable"))?;
    Ok(b)
}

/// 规整用户输入的 key：去空白与连字符、转大写（手输容错）。
pub fn normalize_key(key: &str) -> String {
    key.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .flat_map(|c| c.to_uppercase())
        .collect()
}

/// 规整并校验 6 位数字配对码：去空白与连字符后必须恰好为 6 位 ASCII 数字（`000000`~`999999`）。
pub fn normalize_pair_code(code: &str) -> Option<String> {
    let cleaned: String = code
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    if cleaned.len() == 6 && cleaned.chars().all(|c| c.is_ascii_digit()) {
        Some(cleaned)
    } else {
        None
    }
}

/// 校验用户 ID：1~12 位大小写字母 + 数字，且首字符必须为英文字母。
pub fn is_valid_user_id(user_id: &str) -> bool {
    let s = user_id.trim();
    if s.is_empty() || s.len() > MAX_USER_ID_LEN {
        return false;
    }
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic() && chars.all(|c| c.is_ascii_alphanumeric())
}

/// 校验设备码：1~32 位非空白 ASCII 可打印字符。
pub fn is_valid_device_id(device_id: &str) -> bool {
    let s = device_id.trim();
    !s.is_empty() && s.len() <= MAX_DEVICE_ID_LEN && s.chars().all(|c| c.is_ascii_graphic())
}

/// 生成均匀分布的 6 位数字配对码（`000000`~`999999`）。
fn random_pair_code() -> std::io::Result<String> {
    // 拒绝采样：u32::MAX = 4_294_967_295，取 < 4_294_000_000 保证模 1_000_000 完全均匀。
    const LIMIT: u32 = 4_294_000_000;
    loop {
        let b = random_bytes::<4>()?;
        let v = u32::from_le_bytes(b);
        if v < LIMIT {
            return Ok(format!("{:06}", v % 1_000_000));
        }
    }
}

impl TerminalStore {
    pub fn path() -> PathBuf {
        super::data_dir().join("asr-terminals.json")
    }

    pub fn load() -> Self {
        std::fs::read(Self::path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        super::write_atomic(&Self::path(), &bytes)
    }

    /// 添加终端：生成新 key 与 6 位数字配对码，返回 `(终端编号, key 明文)`。
    /// 若需同时拿到 6 位数字配对码明文，调用 [`Self::create_with_pin`]。
    pub fn create(&mut self, now: i64) -> std::io::Result<(String, String)> {
        let (id, key, _pin) = self.create_with_pin(now)?;
        Ok((id, key))
    }

    /// 添加终端：生成新 key 与 6 位数字配对码，返回 `(终端编号, key 明文, 6 位配对码明文)`。明文均不落盘。
    pub fn create_with_pin(&mut self, now: i64) -> std::io::Result<(String, String, String)> {
        let key = base32(&random_bytes::<16>()?);
        let pin = random_pair_code()?;
        let id = base32(&random_bytes::<5>()?).to_lowercase();
        self.terminals.push(Terminal {
            id: id.clone(),
            key_hash: sha256_hex(&key),
            pin_hash: sha256_hex(&pin),
            created_ts: now,
            ..Default::default()
        });
        Ok((id, key, pin))
    }

    /// FLASH-P2-06：建本机终端（同 [`Self::create`]，带 `local` 标记，清空 `pin_hash` 免配对码）。
    pub fn create_local(&mut self, now: i64) -> std::io::Result<(String, String)> {
        let (id, key) = self.create(now)?;
        if let Some(t) = self.get_mut(&id) {
            t.local = true;
            t.pin_hash.clear();
            t.user_id = DEFAULT_LOCAL_USER_ID.to_string();
        }
        Ok((id, key))
    }

    /// 只认 key（WebSocket 握手时用，此时还不知道设备代码）：存在、未撤销、未过绑定时限。
    pub fn check_key(&self, key: &str, now: i64) -> Result<&Terminal, AuthError> {
        let h = sha256_hex(&normalize_key(key));
        let t = self
            .terminals
            .iter()
            .find(|t| t.key_hash == h)
            .ok_or(AuthError::InvalidKey)?;
        if t.revoked {
            return Err(AuthError::Revoked);
        }
        if !t.bound() && !t.local && now - t.created_ts > BIND_WINDOW_SECS {
            return Err(AuthError::Expired);
        }
        Ok(t)
    }

    /// 兼容旧签名（无 user_id / pair_code，主要供旧测试使用）：当终端未设置 `pin_hash` 时按缺省用户授权。
    pub fn authorize(
        &mut self,
        key: &str,
        device_id: &str,
        device_name: &str,
        now: i64,
    ) -> Result<(String, bool), AuthError> {
        let (id, _uid, changed) =
            self.authorize_full(key, None, device_id, device_name, None, now, false)?;
        Ok((id, changed))
    }

    /// 完整鉴权与绑定：校验 key + 6 位数字配对码（首次绑定时）+ 用户 ID + 设备代码 + 设备名称。
    /// - `require_pin_and_user = true`（生产 WebSocket 路径）：
    ///   - 远程终端必須传合法 `user_id`（1~12 位字母开头+字母数字）；
    ///   - 远程终端首次绑定（`!t.bound()`）且 `t.pin_hash` 非空时，必须传正确的 6 位 `pair_code`；
    ///   - 已绑定终端若再次传入了 `pair_code`，也会校验其正确性；未传则直接放行（已绑定设备免每次重输配对码）。
    /// - 返回 `(terminal_id, effective_user_id, dirty)`。
    #[allow(clippy::too_many_arguments)]
    pub fn authorize_full(
        &mut self,
        key: &str,
        user_id: Option<&str>,
        device_id: &str,
        device_name: &str,
        pair_code: Option<&str>,
        now: i64,
        require_pin_and_user: bool,
    ) -> Result<(String, String, bool), AuthError> {
        let (device_id, device_name) = (device_id.trim(), device_name.trim());
        if !is_valid_device_id(device_id) || device_name.is_empty() {
            return Err(AuthError::MissingDevice);
        }
        let id = self.check_key(key, now)?.id.clone();
        let t = self
            .terminals
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or(AuthError::InvalidKey)?;

        let raw_uid = user_id.map(str::trim).unwrap_or("");
        let effective_uid = if t.local {
            // 本机终端：如果前端传了 user_id，校验格式并采用；如果没传，采用已保存的或默认 "local"。
            if !raw_uid.is_empty() {
                if !is_valid_user_id(raw_uid) {
                    return Err(AuthError::InvalidUserId);
                }
                raw_uid.to_string()
            } else if !t.user_id.is_empty() {
                t.user_id.clone()
            } else {
                DEFAULT_LOCAL_USER_ID.to_string()
            }
        } else if !raw_uid.is_empty() {
            if !is_valid_user_id(raw_uid) {
                return Err(AuthError::InvalidUserId);
            }
            raw_uid.to_string()
        } else if !t.user_id.is_empty() {
            if require_pin_and_user && !t.bound() {
                return Err(AuthError::InvalidUserId);
            }
            t.user_id.clone()
        } else if require_pin_and_user {
            return Err(AuthError::InvalidUserId);
        } else {
            DEFAULT_LOCAL_USER_ID.to_string()
        };

        // 校验 6 位数字配对码（本机终端免验；远程终端首次绑定必验；若后续连接显式传了非空 pair_code 也验）。
        if !t.local && !t.pin_hash.is_empty() {
            let provided_pin = pair_code.map(str::trim).unwrap_or("");
            if (!t.bound() && require_pin_and_user) || !provided_pin.is_empty() {
                let Some(norm_pin) = normalize_pair_code(provided_pin) else {
                    return Err(AuthError::InvalidPairCode);
                };
                if sha256_hex(&norm_pin) != t.pin_hash {
                    return Err(AuthError::InvalidPairCode);
                }
            }
        }

        let mut changed = false;
        if t.local {
            // 本机终端不绑死设备或单一用户，只记最近一次连接的设备与用户。
            if t.device_id != device_id {
                t.device_id = device_id.to_string();
                changed = true;
            }
            if t.user_id != effective_uid {
                t.user_id = effective_uid.clone();
                changed = true;
            }
        } else if !t.bound() {
            t.device_id = device_id.to_string();
            t.user_id = effective_uid.clone();
            changed = true;
        } else {
            if t.device_id != device_id {
                return Err(AuthError::DeviceMismatch);
            }
            if !t.user_id.is_empty() && t.user_id != effective_uid {
                return Err(AuthError::UserMismatch);
            }
            if t.user_id.is_empty() && !effective_uid.is_empty() {
                // 升级兼容：老版本已绑定过 device_id 但当时无 user_id 字段 ⇒ 首次带 user_id 连接时补绑。
                t.user_id = effective_uid.clone();
                changed = true;
            }
        }
        if t.device_name != device_name {
            t.device_name = device_name.to_string();
            changed = true;
        }
        Ok((id, effective_uid, changed))
    }

    pub fn revoke(&mut self, id: &str) -> bool {
        match self.terminals.iter_mut().find(|t| t.id == id) {
            Some(t) if !t.revoked => {
                t.revoked = true;
                true
            }
            _ => false,
        }
    }

    pub fn get(&self, id: &str) -> Option<&Terminal> {
        self.terminals.iter().find(|t| t.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Terminal> {
        self.terminals.iter_mut().find(|t| t.id == id)
    }

    /// 是否还有其它未撤销的终端归属在同一个 `voiceprint_key`（如同一 `user_id`）下。
    pub fn any_active_with_vp_key(&self, vp_key: &str, exclude_terminal_id: &str) -> bool {
        !vp_key.is_empty()
            && self
                .terminals
                .iter()
                .any(|t| !t.revoked && t.id != exclude_terminal_id && t.voiceprint_key() == vp_key)
    }

    /// 清掉从未绑定且已过时限的 key（添加了终端却没去连）。
    pub fn prune_expired(&mut self, now: i64) -> bool {
        let before = self.terminals.len();
        self.terminals
            .retain(|t| t.bound() || t.local || now - t.created_ts <= BIND_WINDOW_SECS);
        self.terminals.len() != before
    }
}

/// 网关自签证书（DER）。网关首次启动生成，设置界面读它算指纹放进二维码。
pub fn cert_path() -> PathBuf {
    super::data_dir().join("asr-cert.der")
}

/// 网关证书私钥（PKCS#8 DER；只有网关读）。
pub fn key_path() -> PathBuf {
    super::data_dir().join("asr-key.der")
}

/// 证书指纹 = 证书 DER 的 SHA-256（十六进制小写）。终端配对时记下，之后每次连接都比对（证书钉扎）。
pub fn fingerprint(cert_der: &[u8]) -> String {
    sha256_hex_bytes(cert_der)
}

/// 读本机证书指纹（网关还没生成过证书 ⇒ `None`）。
pub fn cert_fingerprint() -> Option<String> {
    std::fs::read(cert_path()).ok().map(|d| fingerprint(&d))
}

/// 二维码 / 连接码内容（设计 §8.2）。`lan` / `wan` 为 `主机:端口`；`fp` = 证书 SHA-256 指纹（十六进制）。
pub fn pair_uri(lan: &str, wan: Option<&str>, key: &str, fp: &str) -> String {
    let mut s = format!("flash-asr://pair?v=1&lan={lan}");
    if let Some(w) = wan {
        s.push_str("&wan=");
        s.push_str(w);
    }
    s.push_str("&key=");
    s.push_str(key);
    s.push_str("&fp=");
    s.push_str(fp);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base32_known_vectors() {
        assert_eq!(base32(b""), "");
        assert_eq!(base32(b"f"), "MY");
        assert_eq!(base32(b"foobar"), "MZXW6YTBOI");
        assert_eq!(base32(&[0u8; 16]).len(), 26);
    }

    /// FLASH-P3-C：本机终端不绑设备 —— 先连的是测试客户端，输入法后连也不被拒；只记最近一次的设备。
    #[test]
    fn local_terminal_not_device_locked() {
        let mut st = TerminalStore::default();
        let (id, key) = st.create_local(1000).unwrap();
        let (got, _) = st
            .authorize(&key, "python-ref", "python-ref-client", 1100)
            .unwrap();
        assert_eq!(got, id);
        let (got, changed) = st.authorize(&key, "ime-dev", "输入法", 1200).unwrap();
        assert_eq!(got, id, "换设备也放行");
        assert!(changed, "记下最近一次的设备");
        assert_eq!(st.terminals[0].device_id, "ime-dev");
        let (_, changed) = st.authorize(&key, "ime-dev", "输入法", 1300).unwrap();
        assert!(!changed, "同一设备再来不必保存");
    }

    #[test]
    fn bind_once_then_device_locked() {
        let mut st = TerminalStore::default();
        let (id, key) = st.create(1000).unwrap();
        assert!(st.terminals[0].key_hash != key, "只存哈希");
        // 手输容错：小写 + 空格。
        let typed = key.to_lowercase();
        let (got, changed) = st.authorize(&typed, "dev-A", "手机", 1100).unwrap();
        assert_eq!(got, id);
        assert!(changed, "首次绑定要保存");
        assert_eq!(
            st.authorize(&key, "dev-B", "手机", 1200),
            Err(AuthError::DeviceMismatch),
            "换设备拒绝"
        );
        let (_, changed) = st.authorize(&key, "dev-A", "新名字", 99_999).unwrap();
        assert!(changed, "已绑定不受 10 分钟限制；改名要保存");
        assert_eq!(st.terminals[0].device_name, "新名字");
    }

    #[test]
    fn unbound_key_expires_and_revoke() {
        let mut st = TerminalStore::default();
        let (id, key) = st.create(0).unwrap();
        assert_eq!(
            st.authorize(&key, "d", "n", BIND_WINDOW_SECS + 1),
            Err(AuthError::Expired)
        );
        assert!(st.prune_expired(BIND_WINDOW_SECS + 1));
        assert!(st.terminals.is_empty());
        let (id2, key2) = st.create(0).unwrap();
        assert_ne!(id, id2);
        assert!(st.revoke(&id2));
        assert_eq!(st.check_key(&key2, 1).err(), Some(AuthError::Revoked));
        assert_eq!(
            st.authorize("NOPE", "d", "n", 1),
            Err(AuthError::InvalidKey)
        );
        assert_eq!(
            st.authorize(&key2, "", "n", 1),
            Err(AuthError::MissingDevice)
        );
    }

    #[test]
    fn pair_uri_shape() {
        assert_eq!(
            pair_uri("192.168.1.5:7860", Some("a.com:7860"), "KEY", "ab"),
            "flash-asr://pair?v=1&lan=192.168.1.5:7860&wan=a.com:7860&key=KEY&fp=ab"
        );
        assert_eq!(
            pair_uri("10.0.0.2:7860", None, "K", "f"),
            "flash-asr://pair?v=1&lan=10.0.0.2:7860&key=K&fp=f"
        );
    }

    /// FLASH-P2-06：本机终端不受绑定时限约束、不被清理；撤销照常生效。
    #[test]
    fn local_terminal_never_expires_but_can_be_revoked() {
        let mut st = TerminalStore::default();
        let (id, key) = st.create_local(0).unwrap();
        let later = BIND_WINDOW_SECS * 100;
        assert!(st.check_key(&key, later).unwrap().local);
        assert!(!st.prune_expired(later), "本机终端不被清理");
        assert!(st.revoke(&id));
        assert_eq!(st.check_key(&key, later).unwrap_err(), AuthError::Revoked);
        // 普通终端照旧过期。
        let (_, k2) = st.create(0).unwrap();
        assert_eq!(st.check_key(&k2, later).unwrap_err(), AuthError::Expired);
    }

    #[test]
    fn user_id_and_device_id_and_pair_code_validation() {
        // is_valid_user_id
        assert!(is_valid_user_id("a"));
        assert!(is_valid_user_id("Gavin01"));
        assert!(is_valid_user_id("Abc123456789")); // 12位合法

        assert!(!is_valid_user_id("")); // 空
        assert!(!is_valid_user_id("1abc")); // 数字开头非法
        assert!(!is_valid_user_id("_abc")); // 下划线非法
        assert!(!is_valid_user_id("Abc1234567890")); // 13位超长
        assert!(!is_valid_user_id("a b")); // 含空格非法
        assert!(!is_valid_user_id("中文")); // 非 ASCII

        // is_valid_device_id
        assert!(is_valid_device_id("CPU-9F8A-MB-1234"));
        assert!(is_valid_device_id("A"));
        let valid_32 = "a".repeat(32);
        assert!(is_valid_device_id(&valid_32));

        assert!(!is_valid_device_id(""));
        let invalid_33 = "a".repeat(33);
        assert!(!is_valid_device_id(&invalid_33));
        assert!(!is_valid_device_id("CPU 01")); // 含空格

        // normalize_pair_code
        assert_eq!(normalize_pair_code("123456"), Some("123456".to_string()));
        assert_eq!(normalize_pair_code(" 123-456 "), Some("123456".to_string()));
        assert_eq!(normalize_pair_code("12345"), None); // 5位
        assert_eq!(normalize_pair_code("1234567"), None); // 7位
        assert_eq!(normalize_pair_code("12345a"), None); // 含字母
    }

    #[test]
    fn authorize_full_enforces_pin_and_user_and_shares_across_terminals() {
        let mut st = TerminalStore::default();
        let (id1, key1, pin1) = st.create_with_pin(1000).expect("创建终端1");
        assert_eq!(pin1.len(), 6);
        assert!(pin1.chars().all(|c| c.is_ascii_digit()));

        // 首次绑定（require_pin_and_user = true）：未传 pair_code 或传错 pair_code ⇒ 返回 Err(AuthError::InvalidPairCode)
        let err_no_pin = st.authorize_full(&key1, Some("Gavin01"), "CPU01", "PC", None, 1100, true);
        assert_eq!(err_no_pin, Err(AuthError::InvalidPairCode));

        let err_wrong_pin = st.authorize_full(
            &key1,
            Some("Gavin01"),
            "CPU01",
            "PC",
            Some("000000"),
            1100,
            true,
        );
        assert_eq!(err_wrong_pin, Err(AuthError::InvalidPairCode));

        // 首次绑定：传非法 user_id（如 "123bad" 或空）⇒ 返回 Err(AuthError::InvalidUserId)
        let err_bad_user = st.authorize_full(
            &key1,
            Some("123bad"),
            "CPU01",
            "PC",
            Some(&pin1),
            1100,
            true,
        );
        assert_eq!(err_bad_user, Err(AuthError::InvalidUserId));

        let err_empty_user =
            st.authorize_full(&key1, Some(""), "CPU01", "PC", Some(&pin1), 1100, true);
        assert_eq!(err_empty_user, Err(AuthError::InvalidUserId));

        // 首次绑定：传正确 pin1、合法 user_id = "Gavin01"、device_id = "CPU01"、device_name = "PC" ⇒ 成功返回 Ok((id1, "Gavin01", true))
        let (auth_id, auth_user, changed) = st
            .authorize_full(
                &key1,
                Some("Gavin01"),
                "CPU01",
                "PC",
                Some(&pin1),
                1100,
                true,
            )
            .expect("首次绑定成功");
        assert_eq!(auth_id, id1);
        assert_eq!(auth_user, "Gavin01");
        assert!(changed);

        // 已绑定后第二次调用 authorize_full：不传 pair_code（None）、传相同 user_id = "Gavin01" 和 device_id = "CPU01" ⇒ 成功放行
        let (auth_id2, auth_user2, _) = st
            .authorize_full(&key1, Some("Gavin01"), "CPU01", "PC", None, 1200, true)
            .expect("已绑定终端二次放行");
        assert_eq!(auth_id2, id1);
        assert_eq!(auth_user2, "Gavin01");

        // 已绑定后若换 user_id = "OtherUser" ⇒ 返回 Err(AuthError::UserMismatch)
        let err_user_mismatch =
            st.authorize_full(&key1, Some("OtherUser"), "CPU01", "PC", None, 1200, true);
        assert_eq!(err_user_mismatch, Err(AuthError::UserMismatch));

        // 换 device_id = "CPU02" ⇒ 返回 Err(AuthError::DeviceMismatch)
        let err_dev_mismatch =
            st.authorize_full(&key1, Some("Gavin01"), "CPU02", "PC", None, 1200, true);
        assert_eq!(err_dev_mismatch, Err(AuthError::DeviceMismatch));

        // 再用 create_with_pin(1000) 创建终端 2（代表同一用户的手机），绑定到同一个 user_id = "Gavin01"、device_id = "Phone01"，验证 any_active_with_vp_key("Gavin01", &id1) 为 true（撤销终端 1 时保留该用户声纹）
        let (_id2, key2, pin2) = st.create_with_pin(1000).expect("创建终端2");
        st.authorize_full(
            &key2,
            Some("Gavin01"),
            "Phone01",
            "手机",
            Some(&pin2),
            1300,
            true,
        )
        .expect("终端2绑定同一用户");

        assert!(
            st.any_active_with_vp_key("Gavin01", &id1),
            "即使排除 id1，终端 2 仍然有效归属于 Gavin01，应保留声纹"
        );
    }
}
