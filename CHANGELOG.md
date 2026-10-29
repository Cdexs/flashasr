# CHANGELOG · flashasr

> 任务编号 | 简要说明 | 负责人 | 完成时间

| 编号 | 说明 | 负责人 | 时间 |
| --- | --- | --- | --- |
| FLASH-P1 | **建仓搬家（0.1.0 开发中，未出包、未推送）**（Gavin 10-29「改完直接开发二期」）。从 voice-ime `7c1e6ae` 复制识别服务：两个程序 `flashasr-core`（内核，无界面）/ `flashasr-gateway`（网关）；voice-ime `main.rs` 里识别核心依赖的 77 项由脚本抽到 `src/core/glue.rs`（`include!` 进 crate 根，语义与原处相同）。内核启动即加载本地实时双模型 + 对齐器、总开关缺省开、拉起网关；`--add-terminal` 无界面配对。引擎只剩本地实时 + 在线 FunASR（删 performance / accuracy / qwen-audio / 客户端模式）。依赖锁文件与 voice-ime 对齐（新解析曾拉到 zhconv 0.4.2，繁简表不同，被测试抓到）。隐私泄露检测钩子（提交 / 推送，源自 voice-ime）+ 本项目路径规则（PoC / localtest、声纹、终端 key / 证书）；全历史审计 0 命中（上游 cmake 作者公开邮箱登记白名单）。测试（子 agent）：core 1538 / gateway 42 全过，fmt 0；端到端冒烟：模型 13.4s 加载完、配对、参考客户端识别中文录音出预览 + 终稿 + 逐字时间、加翻译出英文译文 | 主控 / 子 Agent | 2026-10-29 |
| FLASH-P2-01 | **进程结构反转**：前台 `flashasr` 常驻、单实例（锁文件），对外协议 / 配对 / 管理通道都在前台；引擎 `flashasr-engine` 由前台拉起看护（命名管道 + 一次性令牌，标准输入断开即退出）。引擎崩溃 ⇒ 进行中的识别回 `EngineRestarted`，5 秒后重拉；改端口 / 总开关 ⇒ 前台自重启 | 主控 / 子 Agent | 2026-10-29 |
| FLASH-P2-02 | **删在线 FunASR**（Gavin 10-29「只做我们本地实时语音识别服务」）：引擎只剩本地实时；在线 FunASR 由输入法直连阿里（P3 实施） | 主控 / 子 Agent | 2026-10-29 |
| FLASH-P2-04 | `GET /v1/status`（与 WebSocket 同端口同 TLS，先读请求头再决定走 HTTP 还是倒回给 WebSocket）：版本、协议号、引擎在线、本地实时就绪、大模型是否配好、是否暂停；要终端 key。「剩余路数」未做 | 主控 / 子 Agent | 2026-10-29 |
| FLASH-P2-05 | 协议扩展：`feiyin.scene`（场景给输出优化 / 大模型翻译）、`usage.speech_ms`（独立 VAD 算有效语音）、终稿 `wordbook_hits` / `learn_suggestions` / `optimization_skipped`、`POST /v1/profile/voiceprint/reset`（只重置调用方自己的声纹）。「大模型已配置」= 开关开且地址与 key 都填了 | 主控 / 子 Agent | 2026-10-29 |
| FLASH-P2-06 | 本机自动配对：每次启动保证有「本机终端」（key 永不过期、只认回环来源），连接码写 `<用户配置目录>/flashasr/local-pairing.json`；吊销或文件丢失自动重建，改端口沿用 key | 主控 / 子 Agent | 2026-10-29 |
| FLASH-P2-07 | 二期 a 收尾：协议文档补 §1.5 本机配对、§4.1 场景、§5.2 终稿扩展、§5.4 `speech_ms` / `EngineRestarted`、§5.5 HTTP 接口；README 改为新程序名。测试：前台 52 / 引擎 1540 全过，fmt 0；冒烟（词表命中、speech_ms、未配大模型提示、声纹重置与 401、status）全过 | 主控 / 子 Agent | 2026-10-29 |
