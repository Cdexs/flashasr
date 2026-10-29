# flashasr

飞音识别服务。给飞音输入法（Windows / macOS）和其他终端（手机等）提供语音识别，可以跑在本机，也可以跑在另一台电脑上。

## 引擎（只做本地实时；不做在线 FunASR 中转）

| 引擎 | 说明 |
| --- | --- |
| 本地实时 | SenseVoice 边说边出字 + Qwen3-ASR 1.7B 精解 + 强制对齐，本机 GPU / CPU 运行，不联网 |

## 程序

| 程序 | 作用 |
| --- | --- |
| `flashasr` | 前台（常驻、单实例）：对外 WebSocket + HTTP 接口（TLS 1.3 + 证书指纹钉扎）、终端鉴权与配对、本机自动配对；拉起并看护引擎（崩溃 5 秒后重拉）；不加载模型 |
| `flashasr-engine` | 引擎（由前台拉起，不单独运行）：加载模型、识别会话、文字处理；前台退出即退出 |

## 协议

终端接入协议见 `docs/ASR-PROTOCOL.md`（与阿里百炼 `run-task` 同格式）；另有 `GET /v1/status`、`POST /v1/profile/voiceprint/reset`。
参考客户端：`tools/asr-client/feiyin_asr_client.py`。同一台电脑上的终端读 `%APPDATA%\flashasr\local-pairing.json`（macOS 在 `~/Library/Application Support/flashasr/`）免配对。

## 开发

- 编译：`cargo build --bins`；测试：`cargo test --bin flashasr` / `cargo test --bin flashasr-engine`。
- 运行目录需要：模型目录 `models/`、运行库 DLL（sherpa-onnx / onnxruntime / llama.cpp / CTranslate2 / VC++）、规则文件 `*-rules.toml`。
- 无界面配对：服务运行时执行 `flashasr --add-terminal`，打印连接码。
- **提交 / 推送前的隐私泄露检测（必装，每个 clone 一次）**：`git config core.hooksPath scripts/git-hooks`。
  提交时扫新增内容（密钥 / 凭证 / 邮箱等），推送时扫整段待推提交；按路径直接拒绝配置文件、日志、数据库、声纹、
  终端 key / 证书、PoC 与本地临时测试（`localtest/`、`poc_*`）。误报才用 `SECRET_SCAN_SKIP=1` 显式放行（留痕）。
- ⚠️ 不要 `cargo clean`：开发时 `target/debug/models` 是指向模型库的目录联接，清理可能顺着联接删掉模型。

## 状态

0.1.0 开发中。拆分方案见 `docs/SPLIT-PLAN.md`。
