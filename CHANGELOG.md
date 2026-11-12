# CHANGELOG · flashasr

> 任务编号 | 简要说明 | 负责人 | 完成时间

| 编号 | 说明 | 负责人 | 时间 |
| --- | --- | --- | --- |
| PAIR-PIN-USER-WORDBOOK | **6位配对码 + 用户维度词库与声纹共享**：新增6位数字配对码（`pair_code`，SHA-256 校验）与 `pair-terminal` 即时配对验证动作；新增 `user_id` 标识（最长12位字母数字且首字符为字母，`^[A-Za-z][A-Za-z0-9]{0,11}$`，大小写敏感并映射至大小写安全声纹文件 `_u<bitmask>.bin`），服务端按 `user_id` 隔离存储用户词表与系统学习词表（`get-wordbook` / `update-wordbook`，禁止客户端改写系统词），并按 `user_id` 挂载声纹槽位实现跨终端共享；`flashasr-ui` 新增终端弹窗展示6位配对码，终端列表显示 `user_id` 与 `device_id`；同步更新 `docs/API.md`、`docs/ASR-PROTOCOL.md` 与 `tools/asr-client/feiyin_asr_client.py` | 主控 / 子 Agent | 2026-11-13 |
| STOP-RESTART-FIX | **修复管理界面停止服务后再点启动无法启动且引擎不加载**（方案A）：Windows 下阻断 `flashasr-ui` / 自重启 / 引擎子进程继承 `7860` 端口 socket 与 `flashasr.lock` 句柄（`SetHandleInformation` + `CreateProcessW(bInheritHandles=FALSE)`）；`run()` 改为先绑端口（支持 5s 重试）再拉起 `flashasr-engine`，绑端口失败不退出常驻前台；停止/退出清理 `asr-gateway.json`；`server_set_enabled(true)` 检测 `flashasr.lock` 未持有时自动拉起前台 | 主控 / 子 Agent | 2026-11-12 |
| MODELS-PACK | **模型包 Release + 缺模型提示**（Gavin 10-30）：GitHub Release `models-v1` 上传 7z 三分卷（含 LICENSES，NLLB 注明仅限非商业）+ 校验值；管理界面「服务」页模型没放好时显示三步说明、下载地址（点击打开 / 复制）、解压目录（打开 / 复制），清单单一来源 `asr_shared::models` | 主控 | 2026-10-30 |
| SPEED-WORDTIMES-517 | 新参数 `feiyin.word_times`（缺省 true；false ⇒ 松键后最后一窗不强制对齐，终稿快 ~0.25s）；预览帧按 `chinese_script` 转繁简（与终稿一致）。Gavin 定不测试，直接构建出包 | 主控 | 2026-10-30 |
| FLASH-P1 | **建仓搬家（0.1.0 开发中，未出包、未推送）**（Gavin 10-29「改完直接开发二期」）。从 voice-ime `7c1e6ae` 复制识别服务：两个程序 `flashasr-core`（内核，无界面）/ `flashasr-gateway`（网关）；voice-ime `main.rs` 里识别核心依赖的 77 项由脚本抽到 `src/core/glue.rs`（`include!` 进 crate 根，语义与原处相同）。内核启动即加载本地实时双模型 + 对齐器、总开关缺省开、拉起网关；`--add-terminal` 无界面配对。引擎只剩本地实时 + 在线 FunASR（删 performance / accuracy / qwen-audio / 客户端模式）。依赖锁文件与 voice-ime 对齐（新解析曾拉到 zhconv 0.4.2，繁简表不同，被测试抓到）。隐私泄露检测钩子（提交 / 推送，源自 voice-ime）+ 本项目路径规则（PoC / localtest、声纹、终端 key / 证书）；全历史审计 0 命中（上游 cmake 作者公开邮箱登记白名单）。测试（子 agent）：core 1538 / gateway 42 全过，fmt 0；端到端冒烟：模型 13.4s 加载完、配对、参考客户端识别中文录音出预览 + 终稿 + 逐字时间、加翻译出英文译文 | 主控 / 子 Agent | 2026-10-29 |
| FLASH-P2-01 | **进程结构反转**：前台 `flashasr` 常驻、单实例（锁文件），对外协议 / 配对 / 管理通道都在前台；引擎 `flashasr-engine` 由前台拉起看护（命名管道 + 一次性令牌，标准输入断开即退出）。引擎崩溃 ⇒ 进行中的识别回 `EngineRestarted`，5 秒后重拉；改端口 / 总开关 ⇒ 前台自重启 | 主控 / 子 Agent | 2026-10-29 |
| FLASH-P2-02 | **删在线 FunASR**（Gavin 10-29「只做我们本地实时语音识别服务」）：引擎只剩本地实时；在线 FunASR 由输入法直连阿里（P3 实施） | 主控 / 子 Agent | 2026-10-29 |
| FLASH-P2-04 | `GET /v1/status`（与 WebSocket 同端口同 TLS，先读请求头再决定走 HTTP 还是倒回给 WebSocket）：版本、协议号、引擎在线、本地实时就绪、大模型是否配好、是否暂停；要终端 key。「剩余路数」未做 | 主控 / 子 Agent | 2026-10-29 |
| FLASH-P2-05 | 协议扩展：`feiyin.scene`（场景给输出优化 / 大模型翻译）、`usage.speech_ms`（独立 VAD 算有效语音）、终稿 `wordbook_hits` / `learn_suggestions` / `optimization_skipped`、`POST /v1/profile/voiceprint/reset`（只重置调用方自己的声纹）。「大模型已配置」= 开关开且地址与 key 都填了 | 主控 / 子 Agent | 2026-10-29 |
| FLASH-P2-06 | 本机自动配对：每次启动保证有「本机终端」（key 永不过期、只认回环来源），连接码写 `<用户配置目录>/flashasr/local-pairing.json`；吊销或文件丢失自动重建，改端口沿用 key | 主控 / 子 Agent | 2026-10-29 |
| FLASH-P2-07 | 二期 a 收尾：协议文档补 §1.5 本机配对、§4.1 场景、§5.2 终稿扩展、§5.4 `speech_ms` / `EngineRestarted`、§5.5 HTTP 接口；README 改为新程序名。测试：前台 52 / 引擎 1540 全过，fmt 0；冒烟（词表命中、speech_ms、未配大模型提示、声纹重置与 401、status）全过 | 主控 / 子 Agent | 2026-10-29 |
| — | 连接码前缀 `feiyin-asr://` → `flash-asr://`（Gavin 10-30），不认旧前缀 | 主控 / 子 Agent | 2026-10-30 |
| FLASH-P2b-01 | 控制通道扩展：状态带引擎在线 / 本地实时就绪 / 大模型已配好 / 模型加载失败原因；新命令终端列表、重置声纹、配置热重载（引擎新帧 `ReloadConfig`，不打断进行中的识别） | 主控 / 子 Agent | 2026-10-30 |
| FLASH-P2b-02 | 前台托盘（Windows）：打开管理界面 / 暂停接入 / 退出，双击打开界面，提示显示状态；总开关关着也在；退出 / 自重启先收图标 | 主控 / 子 Agent | 2026-10-30 |
| FLASH-P2b-03 | 管理界面 `flashasr-ui`（Tauri + React，版式沿用 voice-ime 设置界面，三语）：服务 / 终端 / 输出优化 / 关于；单实例；大模型配置只改 `config.toml` 的 `[llm]`，连接测试按错误码出文案 | 主控 / 子 Agent | 2026-10-30 |
| FLASH-P2b-04 | Windows 安装脚本 `installer/flashasr.iss`（只按用户安装、开机自启、升级前结束进程、卸载可选删数据保留模型）+ 归集脚本 `scripts/assemble-publish.ps1`（清单与安装脚本逐项对应，缺项报错）；均未运行出包 | 主控 | 2026-10-30 |
