# CHANGELOG · flashasr

> 任务编号 | 简要说明 | 负责人 | 完成时间

| 编号 | 说明 | 负责人 | 时间 |
| --- | --- | --- | --- |
| FLASH-P1 | **建仓搬家（0.1.0 开发中，未出包、未推送）**（Gavin 10-29「改完直接开发二期」）。从 voice-ime `7c1e6ae` 复制识别服务：两个程序 `flashasr-core`（内核，无界面）/ `flashasr-gateway`（网关）；voice-ime `main.rs` 里识别核心依赖的 77 项由脚本抽到 `src/core/glue.rs`（`include!` 进 crate 根，语义与原处相同）。内核启动即加载本地实时双模型 + 对齐器、总开关缺省开、拉起网关；`--add-terminal` 无界面配对。引擎只剩本地实时 + 在线 FunASR（删 performance / accuracy / qwen-audio / 客户端模式）。依赖锁文件与 voice-ime 对齐（新解析曾拉到 zhconv 0.4.2，繁简表不同，被测试抓到）。隐私泄露检测钩子（提交 / 推送，源自 voice-ime）+ 本项目路径规则（PoC / localtest、声纹、终端 key / 证书）；全历史审计 0 命中（上游 cmake 作者公开邮箱登记白名单）。测试（子 agent）：core 1538 / gateway 42 全过，fmt 0；端到端冒烟：模型 13.4s 加载完、配对、参考客户端识别中文录音出预览 + 终稿 + 逐字时间、加翻译出英文译文 | 主控 / 子 Agent | 2026-10-29 |
