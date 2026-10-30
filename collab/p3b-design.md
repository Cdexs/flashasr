# FLASH-P3-B · 输入法「连 flashasr」补全设计（2026-10-30，主控）

> 在 P3-A（删本地引擎）落地后的代码上做。协议见 `docs/ASR-PROTOCOL.md`。

## 1. 请求参数（`run-task`，每段录音按当时设置现算）

| 协议参数 | 取自输入法 | 说明 |
| --- | --- | --- |
| `inverse_text_normalization_enabled` | 恒 `true` | 输入法没有 ITN 开关 |
| `punctuation_prediction_enabled` | `config.punctuation.enabled` | 关 ⇒ 服务端全去标点（与本机「自动标点」关同语义） |
| `translation_enabled` | `translate_requested && config.translation.enabled` | `translate_requested` = 本次按的是翻译热键（按键时已知） |
| `feiyin.output_optimization_enabled` | `config.llm.enabled` | 连 flashasr 时用服务端统一大模型，输入法自己不必配 Key（见 §4） |
| `feiyin.chinese_script` | `config.audio.chinese_script` | 简 / 繁；跟随系统等其它值 ⇒ 不传（服务端按自己的设置） |
| `feiyin.strip_trailing_punct` | `config.punctuation.strip_trailing` | |
| `feiyin.scene` | 场景感知结果 `{kind, style_hint, multiline_safe}` | `config.scene.enabled` 关 ⇒ 不传；窗口标题 / 程序名不传 |
| `vocabulary` | 完整词库数组（已有） | |
| `feiyin.device_id / device_name` | 配对（已有） | |

## 2. 终稿回来要收的东西（`OnlineFinal` 扩字段，只 flashasr 路径填）

- `server_processed: bool` —— true ⇒ 文字已在服务端走完全部处理。
- `translation: Option<String>` —— `output.translations[0].text`。
- `wordbook_hits: Vec<String>`、`learn_suggestions: Vec<String>`、`optimization_skipped: String`（终稿 `output.feiyin`）。
- `speech_ms: Option<u64>` —— `task-finished` 的 `usage.speech_ms`。

## 3. 终稿后处理（`run_pipeline_core` 加 flashasr 分支）

服务端已做：剥标点重打、有效文本、同音、ITN、大模型优化 / 繁简、单位补丁、标点、去句末标点、翻译。输入法**不再重复任何文字处理**，只做：
1. 上屏文字 = 有译文且本次请求翻译 ⇒ 译文；否则终稿。空 ⇒ 按「没听到」处理（同现有 NoSpeech）。
2. 词库命中计数：用服务端 `wordbook_hits`（不再本机重算）。
3. 自动学习：`learn_suggestions` 走现有大模型学习建议的同一入口（阈值规则不变）。
4. 使用统计：`speech_ms` 用服务端值；缺 ⇒ 整段时长。
5. `optimization_skipped == "llm_failed"` ⇒ 沿用 FormatFailed 浮层提示；`"llm_not_configured"` ⇒ 只记日志、不弹窗（每句都弹会打扰），改由设置页「输出优化」旁显示服务端大模型状态（P3-C）。
6. **翻译开关以说完时为准**（实现时发现：翻译热键可在录音中途按下）：`run-task` 发开始时的值，`finish-task` 带 `parameters.translation_enabled` 终值；flashasr 终稿阶段按终值决定翻译与「原文是否单独过大模型」（协议 §4.3）。

直连阿里路径不变（输入法本机规则处理 + 自配大模型）。

## 4. 设置界面

- 连 flashasr 时，「输出优化」开关不要求本机填 Key / 测试通过（大模型在服务端）；说明文案改为「由识别服务的大模型完成」。直连阿里时维持现有门槛。
- 翻译同理（连 flashasr 时由服务端完成）。

## 5. 连接

- 连接码前缀改 `flash-asr://`（`asr_shared/remote.rs` 解析、测试），不认旧前缀。
- 本机自动发现：`RemotePairing` 未配对时读 `<用户配置目录>/flashasr/local-pairing.json`（`uri` 按连接码解析），本机终端；设备代码 / 名称沿用本机身份。供 P3-C 向导「检测到本机识别服务」一键连用。
- 错误码转人话：新增 `EngineRestarted`（「识别服务重启了，请重说」）；`ModelNotReady` 文案改「识别服务模型还在加载，请稍后」。

## 6. 测试

- `run-task` 参数组装：按配置组合断言 JSON。
- 终稿解析：带 / 不带扩展字段、带译文、`usage.speech_ms`。
- 流水线分支：服务端已处理 ⇒ 不调同音 / ITN / 大模型（用计数桩或纯函数拆分断言）。
- 连接码新前缀、本机配对文件读取。
- 端到端冒烟：本机起 flashasr（`target/debug`，端口 7861）+ 输入法调试版连本机，录音文件回放走一遍（子 agent）。
