# P3 输入法变薄 · voice-ime 代码排查（2026-10-30，只读排查结果，行号以当日代码为准）

> 用途：P3 动手前的依赖地图。改动前按行号复核（代码会变）。

## 关键结论

- 要保留的两条路径（连 flashasr / 直连阿里）**隐性依赖本地模型三处**：① 录音时 Silero VAD 开口门控（`qwen_inference.rs:1250-1337`）② 句间接缝用 CT / 日韩标点模型重判（`repair_online_seams`，main.rs:8966 / :15620）③ 使用统计的有效语音时长用 VAD（`effective_speech_ms`，main.rs:15527）。删模型时这三处必须改。
- `src/punctuation/mod.rs`、`src/translation/mod.rs` 混着要保留的纯规则函数，不能整文件删，要先拆。
- `src/asr_service/*`（本机当识别服务的服务端）完全依赖本地模型 ⇒ 整体删（连同 `src/bin/feiyin-asr-gateway/` 与只给它用的 tokio-tungstenite / tokio-rustls / rcgen）。

## 1. AsrModel（`src/transcription/mod.rs:75-98`）

变体：Performance、Accuracy、QwenAudioOnline、FunAsrRealtime、LocalRealtime、FeiyinServer。`from_config` :101；`is_online_streaming` :121；`uses_accuracy_engine` :136。

分岔点：transcription/mod.rs :1896-1921（`Transcriber::new`）、:1926-1964（本地实时加载 + REMOTE_ENGINES）、:1968（VAD 分段器）、:2146-2185（在线非流式回退）、:2216 / :2352、:3410-3455（`build_recognizer`）。
main.rs：:349 `publish_localrt_ready`；`process_controller_events` :7237-7259、:7334、:7404、:7994；:8269 `load_hotwords_for_accuracy`；:8334 `asr_cheap_reload_needed`；:8439 `apply_reload_result`；`spawn_worker_thread`（:8450 起）:8488/8516/8601/8687 加载重载、:8776-8848 在线流式分派（:8819 FeiyinServer）、:9003 本地实时流式（:9108 `run_acc_windows`、:9131 `transcribe_streaming_local`）；:10021 `run_controller_macos` 加载门；:10476 `select_preprocessing_params`；:15283 / :15580 `run_pipeline_core`；:15708 `usage_category`。
其他：`asr_service/supervisor.rs:71`；`config/mod.rs:203` 新装默认 `"local_realtime"`（**必须改**）、:723/822 accuracy 迁移；`src-tauri/src/config.rs:147/429/468` 配置镜像。

## 2. 连远程识别服务（FeiyinServer）

入口 main.rs:8819 → `qwen_inference::transcribe_streaming_feiyin`（:1031）→ `realtime_session`（:1114）；非流式回退 mod.rs:2148。
模块：`transcription/feiyin_remote.rs`（建连：内外网同时试 TCP、TLS 指纹钉扎、Bearer key）、`asr_shared/remote.rs`（`RemotePairing::load()`，`asr-remote.json`）、`asr_shared/tls_pin.rs`、`asr_shared/pairing.rs`。
发送参数（`build_run_task_message`，qwen_inference.rs:251）：format、sample_rate、language_hints=[zh,en,ja,ko]、semantic_punctuation_enabled=false、max_sentence_silence 默认；`vocabulary` = `load_wordbook_word_array`（main.rs:8821）完整数组；`parameters.feiyin={device_id, device_name}`（`feiyin_remote.rs` `run_task_ext`）；顶层 ITN=true、punct=true、translation=false（`text_params`，qwen_inference.rs:1183-1190）；model 固定 `"feiyin-local-realtime"`。
**P3 要补**：flashasr 协议扩展参数（输出优化 / 翻译 / 繁简 / 去句末标点 / scene）由输入法设置传；终稿 `wordbook_hits` / `learn_suggestions` 落本机词库；`usage.speech_ms` 记统计；连接码前缀 `flash-asr://`；本机自动配对文件 `%APPDATA%\flashasr\local-pairing.json`。
终稿回来后本机：`repair_online_seams`（用 CT / jako 模型）→ `run_pipeline_core(from_online_streaming=true)`（跳过 ITN 主通道）。

## 3. 直连阿里 FunASR

入口 main.rs:8832 → `transcribe_streaming_realtime`（qwen_inference.rs:992），共用 `realtime_session`，建连 `connect_dashscope`（:1061）；非流式回退 `transcribe_streaming`（:717）。
配置（config/mod.rs:147-188）：`asr_online_api_key`、`asr_online_url`、`asr_online_model`（`resolve_online_model_id` :283、`default_online_model_for_family` :238）、`asr_online_max_sentence_silence`、`asr_online_semantic_punctuation_enabled`；`asr_local_realtime_unlocked` 可删。vocabulary 用 `load_wordbook_vocabulary`（带权重对象）。

## 4. 终稿后文字处理（`run_pipeline_core`，main.rs:15161）

| 序 | 步骤 | 位置 | 依赖模型 |
| --- | --- | --- | --- |
| 0 | 接缝标点重判（在线） | `repair_online_seams` | 是（CT / jako） |
| 1 | 有效文本判定 | `text_normalizer::is_effective_text` :15336 | 否 |
| 2 | 同音纠错 | `homophone::apply_homophone_fix` :15348 | 否 |
| 3 | ITN 主通道（在线跳过） | `itn::normalize_numbers` :15362-15369 | 否 |
| 4 | 大模型优化 / 翻译 | `run_llm_stage` :15723 | 翻译回退离线 |
| 5 | ITN 补丁 | `itn::normalize_unit_symbols_only` :15450 | 否 |
| 6 | 本地标点 | `apply_local_punctuation` :15926（:15953 CT、:15940 jako） | 是 |
| 7 | 去口水词（没走大模型时） | `apply_filler_strip` :16012 | 否 |
| 8 | 去句末 / 全部标点 | `punctuation::apply_l2_postprocess` :15491 | 否 |
| 9 | 使用统计 | `effective_speech_ms` :15527 | 是（VAD） |

`run_llm_stage`：翻译 :15746 先 `optimize_and_translate`，失败 / 未开大模型退 `try_nllb_translate`（:16207，离线 CT2）；优化 :15829 `optimize`；繁简在大模型之后（:15853/15860/15869，zhconv）。

## 5. 只为本地模型存在（可整删）

`transcription/`：llama_asr.rs、local_stream.rs、vad.rs、speaker.rs、char_times.rs、poc_*.rs、replay436_tests.rs；mod.rs 大部分（保留 NoSpeechError、在线分支、load_wordbook_vocabulary / word_array、strip_punctuation、model_dir）。`src/localrt_session.rs`、`punctuation/jako.rs`、`native/llama_asr/`、`src/asr_service/*`、`src/bin/feiyin-asr-gateway/`、`src-tauri/src/asr_server.rs` 前 7 个服务端命令（:66-212）+ `ui/src/pages/AsrServer.tsx`、`src/main - 副本.rs`（遗留副本）。
**拆分**：`punctuation/mod.rs` 只删 `PunctuationEngine`（:291-357），保留 :14-290 纯规则（itn.rs:2712 用 `is_punctuation`）；`translation/mod.rs` 只留 `derive_translation_target`（:127）。
main.rs 引用点：`cached_punctuation` / `PunctuationEngine::new` :8542/8960/9052/9508/9586；`cached_translation` :15169/:15736；`try_nllb_translate` :15752/:16207；`apply_local_punctuation`、`repair_online_seams`；`effective_speech_ms` :15527；本地实时整块 :9003-9600；`llama_asr::DecodeAssist` :10586；`voiceprint_dropped_all` :10611；`asr_service::supervisor` :601/635/7138/7144/9988/10410；本地引擎测试模块（:10767 起）。`audio/mod.rs:3013/3108` 测试用 `sherpa_onnx::Wave::read`。

## 6. 构建

Cargo.toml 删：sherpa-onnx、ort、ctranslate2、ctranslate2-sys（含 Windows dnnl / openmp）、sentencepiece；`[patch.crates-io]` cmake / ctranslate2-sys / esaxx-rs；`[build-dependencies] cc`。build.rs :2-18 llama shim 删，winres 留，macOS rpath 可去。`.cargo/config.toml`（只有 SHERPA_ONNX_LIB_DIR）可删。`build.bat:7-39` DLL 复制；`installer/voice-ime.iss:82-98` 模型运行库 DLL（:101-105 VC 运行库待确认是否还需要）。可删目录：vendor/sherpa-onnx、vendor/llama-runtime、vendor/whisper-rs-sys、sherpa-onnx-lib/、patches/、scripts/fetch-llama-runtime.ps1、scripts/fetch-sherpa-onnx.ps1、models/（**删目录属不可逆，先存清单单独成轮**）。

## 7. 设置界面

`ui/src/pages/Voice.tsx`：模型下拉 :236-244（留 fun_asr_realtime、feiyin_server）；默认 'local_realtime'（:26/:40）要改；模型下载提示 :47、:79-84、:171-188、:204、:250-291；描述 :194-199；「自动标点」开关 :360-368（删 CT 后语义要定）。src-tauri：`check_accuracy_model_ready`（main.rs:112-140）删；`test_qwen3_asr_connection`（:98）留；`asr_remote_*`（asr_server.rs:273-351）+ `FeiyinServerPairing.tsx` 留。

## 8. 周边功能

加载中不弹录音窗（LOCALRT_ENGINE_READY，main.rs:344-351，:7404-7410 / :10021）随本地实时删；有效语音时长（mod.rs:1368）⇒ 连 flashasr 用服务端 `usage.speech_ms`，直连阿里改整段时长（口径变化要说明）；声纹 / 近场门随本地实时删；在线 VAD 开口门控删后建连即发音频（阿里计费略增）。

## 9. macOS

`src/platform/macos/*` 无模型引用；main.rs:10021 加载门；jako.rs:52-68 / llama_asr.rs:172-187 dylib 查找；`scripts/build-macos.sh` :36、:106-118、:151-152；`scripts/env-macos.sh:4-25`；`scripts/setup-macos.sh:33-55`；build.rs:50-54 rpath。
