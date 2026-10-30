//! FLASH-P2b-03：大模型统一配置（服务端所有终端共用，与终端用户无关）。
//!
//! - 读写 `config.toml` 的 `[llm]` 一节（`toml_edit`：其余内容、注释、顺序原样保留）。`config.toml` 由引擎第一次
//!   启动时生成；还没有 ⇒ 报 `config_missing`（不自己造半份配置：引擎要的是完整配置）。
//! - 保存后经控制通道让引擎热重载（不打断进行中的识别）；服务没在运行 ⇒ 下次启动自然读到。
//! - 连接测试照 voice-ime 设置界面做法（发一句「Reply with OK only.」），失败回错误码，界面按码取文案。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

use crate::asr_shared;
use crate::asr_shared::control::{self, Request};

const TIMEOUT: Duration = Duration::from_secs(6);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

pub const ERR_CONFIG_MISSING: &str = "config_missing";
pub const ERR_IO: &str = "io";

/// 与引擎 `src/config/mod.rs` 的 `LlmConfig` 同字段。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LlmConfig {
    pub api_url: String,
    pub api_key: String,
    pub model: String,
    pub enabled: bool,
    pub connectivity_verified: bool,
}

fn config_path() -> PathBuf {
    asr_shared::data_dir().join("config.toml")
}

fn str_of(t: &toml_edit::Table, k: &str) -> String {
    t.get(k)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn bool_of(t: &toml_edit::Table, k: &str) -> bool {
    t.get(k).and_then(|v| v.as_bool()).unwrap_or(false)
}

/// 从 `config.toml` 文本取 `[llm]`（纯函数，便于单测）。没有这一节 ⇒ 全空。
pub fn parse_llm(text: &str) -> Result<LlmConfig, String> {
    let doc: toml_edit::DocumentMut = text.parse().map_err(|_| ERR_IO.to_string())?;
    let Some(t) = doc.get("llm").and_then(|i| i.as_table()) else {
        return Ok(LlmConfig::default());
    };
    Ok(LlmConfig {
        api_url: str_of(t, "api_url"),
        api_key: str_of(t, "api_key"),
        model: str_of(t, "model"),
        enabled: bool_of(t, "enabled"),
        connectivity_verified: bool_of(t, "connectivity_verified"),
    })
}

/// 把 `[llm]` 写进 `config.toml` 文本（纯函数）：只改这五个键，其余原样。
pub fn apply_llm(text: &str, c: &LlmConfig) -> Result<String, String> {
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|_| ERR_IO.to_string())?;
    if !doc.contains_table("llm") {
        doc["llm"] = toml_edit::table();
    }
    let t = &mut doc["llm"];
    t["api_url"] = toml_edit::value(c.api_url.trim());
    t["api_key"] = toml_edit::value(c.api_key.trim());
    t["model"] = toml_edit::value(c.model.trim());
    t["enabled"] = toml_edit::value(c.enabled);
    t["connectivity_verified"] = toml_edit::value(c.connectivity_verified);
    Ok(doc.to_string())
}

#[tauri::command]
pub fn llm_get() -> Result<LlmConfig, String> {
    let text =
        std::fs::read_to_string(config_path()).map_err(|_| ERR_CONFIG_MISSING.to_string())?;
    parse_llm(&text)
}

#[tauri::command]
pub fn llm_save(config: LlmConfig) -> Result<(), String> {
    let path = config_path();
    let text = std::fs::read_to_string(&path).map_err(|_| ERR_CONFIG_MISSING.to_string())?;
    let out = apply_llm(&text, &config)?;
    asr_shared::write_atomic(&path, out.as_bytes()).map_err(|_| ERR_IO.to_string())?;
    // 服务没在运行就算了：引擎下次启动读到的就是新配置。
    let _ = control::call(&Request::ReloadConfig);
    Ok(())
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: [Message<'a>; 1],
    max_tokens: u32,
    stream: bool,
    /// 关闭推理模式（SiliconFlow / Qwen3 系）；DeepSeek 靠下面的 `thinking`。不认识的服务商会忽略。
    enable_thinking: bool,
    thinking: Thinking,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct Thinking {
    #[serde(rename = "type")]
    kind: &'static str,
}

fn chat_url(api_url: &str) -> String {
    let base = api_url.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{base}/chat/completions")
    }
}

/// 失败码：`incomplete`（没填全）/ `unreachable`（连不上 / 超时）/ `auth`（401 / 403）/
/// `bad_request`（其它 4xx，多半是模型名不对）/ `server`（5xx 等）/ `bad_response`（回的不是聊天格式）。
#[tauri::command]
pub async fn llm_test(config: LlmConfig) -> Result<(), String> {
    if config.api_url.trim().is_empty()
        || config.api_key.trim().is_empty()
        || config.model.trim().is_empty()
    {
        return Err("incomplete".into());
    }
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(TIMEOUT)
        .build()
        .map_err(|_| ERR_IO.to_string())?;
    let body = ChatRequest {
        model: config.model.trim(),
        messages: [Message {
            role: "user",
            content: "Reply with OK only.",
        }],
        max_tokens: 5,
        stream: false,
        enable_thinking: false,
        thinking: Thinking { kind: "disabled" },
    };
    let resp = client
        .post(chat_url(&config.api_url))
        .bearer_auth(config.api_key.trim())
        .json(&body)
        .send()
        .await
        .map_err(|_| "unreachable".to_string())?;
    let status = resp.status().as_u16();
    match status {
        200..=299 => {}
        401 | 403 => return Err("auth".into()),
        400..=499 => return Err("bad_request".into()),
        _ => return Err("server".into()),
    }
    let v: serde_json::Value = resp.json().await.map_err(|_| "bad_response".to_string())?;
    if v.get("choices").and_then(|c| c.as_array()).is_some() {
        Ok(())
    } else {
        Err("bad_response".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_keeps_other_content() {
        let src = "# 注释\nui_language = \"Chinese\"\n\n[llm]\napi_url = \"a\"\napi_key = \"\"\nmodel = \"m\"\nenabled = false\n\n[audio]\nasr_model = \"local_realtime\"\n";
        let c = LlmConfig {
            api_url: " https://x/v1 ".into(),
            api_key: "k".into(),
            model: "m2".into(),
            enabled: true,
            connectivity_verified: true,
        };
        let out = apply_llm(src, &c).unwrap();
        assert!(
            out.starts_with("# 注释\nui_language = \"Chinese\"\n"),
            "{out}"
        );
        assert!(out.contains("[audio]\nasr_model = \"local_realtime\"\n"));
        let back = parse_llm(&out).unwrap();
        assert_eq!(back.api_url, "https://x/v1", "首尾空白去掉");
        assert_eq!(back.model, "m2");
        assert!(back.enabled && back.connectivity_verified);
    }

    #[test]
    fn missing_llm_section_reads_empty_and_is_added() {
        assert_eq!(parse_llm("a = 1\n").unwrap(), LlmConfig::default());
        let out = apply_llm("a = 1\n", &LlmConfig::default()).unwrap();
        assert!(out.contains("[llm]"));
    }

    #[test]
    fn chat_url_forms() {
        assert_eq!(chat_url("https://a/v1/"), "https://a/v1/chat/completions");
        assert_eq!(
            chat_url("https://a/v1/chat/completions"),
            "https://a/v1/chat/completions"
        );
    }
}
