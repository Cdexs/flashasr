# FLASH-P3-C · 首次连接向导 + 设置页连接方式（2026-10-30，主控定稿）

> 代码在 voice-ime。依赖：P3-A（只剩直连阿里 / 连 flashasr）、P3-B（`RemotePairing::load_for_use` 本机自动配对、`local_flashasr_pairing`）。
> Gavin 原话：「暂时还不存在用户升级的问题。所以第一次使用的时候应该弹出配置连接服务的一个向导」。

## 1. 什么时候弹向导

- 主程序启动时判一次「连接能不能用」（纯函数，不新增配置项）：
  - `asr_model = feiyin_server`：`RemotePairing::load()` 已配对，或本机 flashasr 配对文件存在（`local_flashasr_pairing().is_some()`）⇒ 可用；
  - `asr_model = fun_asr_realtime`：`asr_online_api_key` 非空 ⇒ 可用；
  - 否则 ⇒ 不可用 ⇒ 拉起设置界面并直接打开向导（设置界面进程加启动参数，如 `--wizard`）。
- 有本机 flashasr 就**不弹**（P3-B 已自动连上，零配置）。Windows 与 macOS 控制器都要接（macOS 侧写好、在 MACOS-HANDOFF 记「未编译验证」）。
- 用户直接打开设置界面时，若连接不可用，「语音」页顶部显示同一向导入口（一张提示卡 +「设置连接」按钮），不强制弹。

## 2. 向导内容（设置界面里的模态对话框，沿用现有 `modal-overlay / modal-dialog` 与 card 样式）

三选一：
1. **本机识别服务**：检测到本机 flashasr（新命令读本机配对文件）⇒ 显示「已检测到本机的 flashasr」+「使用」按钮（写 `asr_model=feiyin_server`，配对由 `load_for_use` 自动处理；可顺手调一次连接测试显示结果）。没检测到 ⇒ 灰掉并提示「本机未安装 flashasr」。
2. **连另一台电脑的识别服务**：粘贴连接码（复用现有 `asr_remote_pair_code` / `FeiyinServerPairing` 的逻辑与校验错误文案）⇒ 成功后测试连接（`asr_remote_test`）⇒ 写 `asr_model=feiyin_server`。
3. **直连阿里云**：填 API Key（复用现有测试命令 `test_qwen3_asr_connection`）⇒ 通过后写 `asr_model=fun_asr_realtime` + key。
- 底部「稍后设置」可关；关了之后下次启动仍会判、仍会弹（直到连接可用）。
- 完成后关闭向导、刷新配置（主程序读配置文件的方式照现有设置保存流程）。

## 3. 设置页

- 「语音」页：连接方式两项（已有）；连 flashasr 时显示当前连的是「本机」还是哪台电脑（lan / wan）与连接测试；直连阿里时显示 Key 与测试（已有）。
- 「输出优化」页：
  - 连 flashasr 时：开关**不要求**本机填 Key / 通过测试（大模型在服务端统一配置）；API 配置卡片收起或置灰并说明「连识别服务时由识别服务的大模型完成，这里的配置只在直连阿里时使用」；旁边显示服务端大模型状态：已配置 / 未配置（未配置 ⇒ 提示「识别服务没有配置大模型，开了也会跳过」）。
  - 服务端状态来源：新命令调 flashasr `GET /v1/status`（与 WebSocket 同端口同 TLS，**先钉扎指纹再发 key**，`Authorization: Bearer <key>`，返回 JSON：`engine_online / local_realtime_ready / llm_configured / paused / version`）。在 `src/asr_shared/tls_pin.rs` 照 `check_access` 的写法加 `fetch_status`（读完头部与 `Content-Length` 正文，解析 JSON；连不上 / 401 返回错误码）。
  - 直连阿里时：维持现有门槛（要本机 Key + 测试通过）。
- 「翻译」相关开关同理：连 flashasr 时不要求本机大模型 Key。

## 4. 约束

- 不新增 config.toml 字段（向导是否弹由现状推导）。
- 界面文案全部走 i18n 三份（zh-Hans / zh-Hant / en），主程序侧文案走 `src/i18n.rs` 三份；禁硬编码、禁内部错误原文上屏（按错误码取文案）。
- 统一风格：复用 `section-title`、`card`、`modal-*`、`btn-*`、`toggle-*` 等现有样式类，不自造组件样式。
- 跨平台：Windows / macOS 都接；macOS 不能编译验证的写进汇报。
- 测试：新增纯函数（连接可用判定、status 解析）的单测；前端组件按现有 vitest 写法补用例（向导三条路径的按钮可用性、输出优化页在两种连接方式下的门槛差异）。
- 不改版本号、不 commit、不出包、不 cargo clean、不 --release、不设 SHERPA_ONNX_LIB_DIR、不停 feiyin-ime 进程；不用 PowerShell Set-Content 写 UTF-8 文件。
