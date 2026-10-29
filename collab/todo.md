# flashasr · 待办

> 只放待办。方案 `docs/SPLIT-PLAN.md`（DEC-114，决策记录在 voice-ime `collab/decisions.md`）。
> 版本号 / 出包 / push 一律等 Gavin 指令。测试与构建派低成本子 agent。

## ✅ FLASH-P1 · 建仓搬家（10-29 完成）（Gavin 10-29「改完直接开发二期」）

> 方案 §8 P1：把识别服务从 voice-ime 搬到本仓库并能独立编译、测试通过；删 performance / accuracy / qwen-audio。
> **搬法（主控 10-29 定）**：从 voice-ime `7c1e6ae` **复制**，voice-ime 原样不动（其 `main.rs` 有大量按源码逐字检查的测试，
> 原地抽模块牵动面大；P3 输入法本就整块删除识别代码）。⚠️ 过渡期（P1~P3）两边同步修改。

| 单号 | 子项 | 状态 |
| --- | --- | --- |
| FLASH-P1-01 | 工程骨架：`Cargo.toml`（flashasr 0.1.0，两个程序 `flashasr-core` / `flashasr-gateway`）、`build.rs`（llama.cpp 适配层 + 图标版本资源）、`.cargo/config.toml`；体积小的补丁 / 原生代码复制，大的预编译库（sherpa-onnx / llama-runtime / ctranslate2 补丁）暂引用 voice-ime 目录 | ✅ |
| FLASH-P1-02 | 复制模块：`asr_service` `asr_shared` `transcription` `localrt_session` `punctuation` `itn` `homophone` `text_normalizer` `translation` `llm` `scene` `config` 等；网关程序；规则 toml | ✅ |
| FLASH-P1-03 | `src/core/glue.rs`（`include!` 进 crate 根，脚本 `localtest/extract_glue.py` 生成，共 79 项）：从 voice-ime `main.rs` 复制识别核心依赖的约 45 项（精解窗口规划 / 解码、预览回灌合成、`PipelineEvent` / `AccInput`、标点服务、去口水重复等）及其传递依赖 | ✅ |
| FLASH-P1-04 | `flashasr-core` 主程序（无界面）：启动加载本地实时双模型 + 对齐器 → 登记 → 看护线程（起 IPC + 拉起网关）→ 常驻；`--add-terminal` 无界面配对（打印连接码） | ✅ |
| FLASH-P1-05 | 删 performance / accuracy / qwen-audio：`AsrModel` 只留本地实时 + 在线 FunASR，删其加载 / 迁移 / 测试代码 | ✅ |
| FLASH-P1-06 | 测试（子 agent）：搬来的测试全过；Python 参考客户端对 `flashasr-core` 冒烟 | ✅ |
| FLASH-P1-07 | 文档：README / CLAUDE.md 入口、CHANGELOG、logs；voice-ime 侧记录 | ✅ |

## 🔄 FLASH-P2a · 服务能力（Gavin 10-29「继续」；方案 §8 P2 拆成 P2a 服务能力 + P2b 管理界面 / 安装包）

| 单号 | 子项 | 状态 |
| --- | --- | --- |
| FLASH-P2-01 | 进程结构反转：前台 `flashasr`（原网关，常驻、单实例）拉起并看护引擎 `flashasr-engine`（原内核）；引擎先开 IPC 再后台加载模型，前台一断（标准输入 EOF）即退；引擎崩溃 ⇒ 前台 5s 内重启重连，进行中的会话回 `task-failed EngineRestarted`；端口 / 总开关改动前台进程内跟上；`--add-terminal` 移到前台 | ✅ |
| FLASH-P2-02 | 删在线 FunASR：`AsrModel` 只剩本地实时（不做中转，Gavin 10-29：「我们自己后端的 FlashASR 就不保留直连 FunASR 的能力了，不做这个中转了，只做我们本地实时语音识别服务。」）；`run-task` `model` 忽略 | ✅ |
| ~~FLASH-P2-03~~ | ~~在线 FunASR 转发~~ —— **取消**（同上） | — |
| FLASH-P2-04 | `GET /v1/status`：协议版本、已开引擎、本地实时是否加载完、剩余路数、是否配了大模型 | ✅ 剩余路数未做（引擎侧无此查询，留作后续） |
| FLASH-P2-05 | 协议扩展：`feiyin.scene`（场景给输出优化）、`usage.speech_ms`、终稿 `wordbook_hits` / `learn_suggestions`、未配大模型时的提示字段、`POST /v1/profile/voiceprint/reset` | ✅ 冒烟：词表命中、speech_ms、未配大模型提示、声纹重置（401 无 key）全过 |
| FLASH-P2-06 | 本机自动配对：首次运行自动建「本机」终端，配对信息写到当前用户目录（只本人可读），供输入法自动发现 | ✅ |
| FLASH-P2-07 | 测试（子 agent）+ 端到端冒烟 + 文档 | ✅ 协议文档 / README 同步；前台 52 / 引擎 1540 全过 |
| FLASH-P2-08 | **详细 API 文档**（Gavin 10-29「Flashasr 开发完毕，要生成一份详细的 api 文档，方便调用者查阅和接入开发」）：在 `docs/ASR-PROTOCOL.md` 基础上出完整接入文档 —— 连接与配对、鉴权、`run-task` 全部参数与默认值、结果 / 译文 / 逐字时间格式、错误码全表与处理建议、`/v1/status`、声纹接口、时序图、多语言调用示例（Python / JavaScript / Kotlin / Swift 片段）、版本兼容规则。flashasr 功能开发完毕后做 | ⬜ |

## ⏳ 后续（方案 §8）

- P2 flashasr 独立可用：前台 / 引擎双进程、`/v1/status`、管理界面、本机配对文件、安装包。
- P3 输入法变薄。**⚠️ 输入法保留直连阿里 FunASR**（Gavin 10-29 最终定：「剥离出的前端输入法要保留直连阿里 FunASR 服务的能力。也就是说，前端输入法既可以直连阿里的 FunASR 服务，也可以连接我们自己部署的 FlashASR 服务。先记下来，后面改前端语音输入法的时候再实施。」）：直连 + 连 flashasr 两条路并存，直连模式下哪些处理环节可用随 P3 方案定。P4 发版。
- FLASH-P1 遗留：连接码前缀已改为 `flash-asr://`（Gavin 10-30；P3 输入法按新前缀解析，不认旧前缀）；单实例保护（两个内核会抢端口 / 管道）P2 做。
- 公开仓库首次推送代码前：把引用 voice-ime 的预编译库 / 补丁改为本仓库自带或下载脚本（公开仓库不能依赖兄弟目录）。
