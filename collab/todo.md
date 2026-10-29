# flashasr · 待办

> 只放待办。方案 `docs/SPLIT-PLAN.md`（DEC-114，决策记录在 voice-ime `collab/decisions.md`）。
> 版本号 / 出包 / push 一律等 Gavin 指令。测试与构建派低成本子 agent。

## 🔄 FLASH-P1 · 建仓搬家（Gavin 10-29「改完直接开发二期」）

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
| FLASH-P1-06 | 测试（子 agent）：搬来的测试全过；Python 参考客户端对 `flashasr-core` 冒烟 | ⬜ |
| FLASH-P1-07 | 文档：README / CLAUDE.md 入口、CHANGELOG、logs；voice-ime 侧记录 | ⬜ |

## ⏳ 后续（方案 §8）

- P2 flashasr 独立可用：前台 / 引擎双进程、FunASR 转发、`/v1/status`、管理界面、本机配对文件、安装包。
- P3 输入法变薄。P4 发版。
- 公开仓库首次推送代码前：把引用 voice-ime 的预编译库 / 补丁改为本仓库自带或下载脚本（公开仓库不能依赖兄弟目录）。
