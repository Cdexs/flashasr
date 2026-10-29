// FLASH-P1-03：从 voice-ime `src/main.rs`（`7c1e6ae`）原样复制的识别核心依赖项。
// 由 `src/core/main.rs` 以 `include!` 插入 crate 根（与 voice-ime 同在根上 ⇒ 可见性 / 私有字段语义逐位相同）。
// ⚠️ 过渡期（P1~P3）与 voice-ime 两边同步修改；P3 输入法删除识别代码后只剩这一份。
// 生成：`py -3 localtest/extract_glue.py localtest/glue_names.txt src/core/glue.rs`（勿手改，改名单重生成）。

#[derive(Debug, Clone)]
enum PipelineEvent {
    RecordingStarted,
    /// OVERLAY-051-E: 流式 ASR 已启动但尚未收到第一个文本，overlay 显示占位提示。
    StreamingIdle,
    /// ASR-038-B: 流式 ASR 增量文本（display_text 全量，供 overlay 整段覆盖）
    /// OVERLAY-051-G: 附带 word timings 供时间戳驱动揭示
    /// OVERLAY-075: 附带产生该事件的录音会话代际。跨 session 迟到包（上一 session 的
    /// finalize 拖尾期间用户新开会话）按代际不匹配丢弃，见消费侧 :3682 起。
    StreamingText(
        u64,
        String,
        Vec<crate::transcription::qwen_inference::WordTiming>,
        // STREAM-SV-463：显示文本**末尾**尚未稳定的字数（本地实时模拟流式；浮窗画浅色）。在线档恒 0。
        usize,
    ),
    Processing(String),
    Done,
    Cancelled,
    FocusLost(String),
    Error(String),
    /// FORMAT-LLM-001-CORE (DEC-031-③): LLM 格式化失败，原文已注入兜底。
    /// overlay 显示 2500ms 提示后自动复位 tray 到 Idle（防卡在"处理中"）。
    FormatFailed,
    /// BUG-119: 「用户没说话」信息提示（Gavin BUILD-118 端测第 1 项）。
    /// 无 payload：文案由显示侧查 i18n（no_speech_hint），照 FormatFailed 先例。
    /// 产出源：transcription::NoSpeechError 类型化错误在 worker 边界下探转成本事件，
    /// 新增第三个产出源只需复用该类型，无需改任何分类器。
    NoSpeech,
    /// LOCAL-RT-ENGINE-239-B: 本地流式档位双模型加载提示（Info 态，文案携带在 payload）。
    Info(String),
    /// LOCAL-RT-ENGINE-239-B: 本地流式档位模型缺失/加载失败 → 直接报错不降级（DEC-067 附则一）。
    /// 与 Error 的区别：**不过 convert_to_friendly_error**，payload 即 i18n 文案原样上屏；
    /// 「具体缺哪个模型」的内部原文只进日志（10-14 i18n 收口）。
    ModelUnavailable(String),
    /// LOCALRT-FIRSTCHAR-282: 本地流式**松手 flush 的最终预览全文**。
    /// 🔴 专用通道：`STREAMING_STOPPED` + `should_ignore_streaming_text` 是本地/在线共用 latch，
    /// 动它会影响在线档 + 可能复活 OVERLAY-043 闪烁 ⇒ **新开本事件，只有本地档发/只有本地档消费**，
    /// 在线档代码路径结构上不变（它永不发也永不收本事件）。
    /// 消费：在 `RecordingWithText` 态下展示收尾预览（不 auto-close、不改状态机）。
    StreamingFinalPreview(String),
    /// ACC-PREVIEW-REFLOW-325：本地实时档 **accuracy 分片的权威文本回灌预览**。
    ///
    /// 296/307 已证伪「在流式模型上补尾字」（`gained` 恒 0）；本单换层面：298 的 accuracy
    /// 分片结果在录音过程中陆续回来，用它对**已完成片**的覆盖区做权威替换，尾字自然补齐。
    ///
    /// 字段：`generation` 代际（防串场，同 `StreamingText`）；`seg_index` 该片序号
    /// （**单调键**，worker 按序产生、构造上严格递增）；`committed_len` 派发当刻浮层已显示
    /// 文本的**字符数**（回灌边界）；`has_hole` 本片覆盖范围内是否含**无法填补**的 accuracy
    /// 失败片（344-G 后：失败片一律用该片**流式文本填补**，故 `has_hole` 只在流式也为空等
    /// 真·无法填补时为 true；含则**不得回灌**，否则会把带洞文本灌上浮层）；
    /// `acc_text` 覆盖区内的权威文本（含流式填补，**连续无洞**）。
    ///
    /// 🔴 只有 LocalRealtime 会发；在线/批处理结构上永不发（同 282 专用通道先例）。
    PreviewReflow {
        generation: u64,
        seg_index: usize,
        /// FIX-PREVIEW-STALE-AND-COLLAPSE-375（A）：**滑窗权威快照自有**的单调序号（仅
        /// `replace_all=true` 时给；发送方 = 滑窗 acc worker 的递增计数器，与 `OrderedReflow`
        /// 的快照顺序一致）。过期判据用它而非 `seg_index` —— 后者是切片下标，**会重复**（窗口间
        /// 并发 + 收尾 drain 可让多窗撞同一下标）⇒ 会把后到的完整文本误判成 stale。
        reflow_seq: Option<usize>,
        committed_len: usize,
        /// FIX-NEARFIELD-BY-SEGMENT-AND-PREVIEW-389（D3）：`committed_len` 是否为**准确边界**。
        /// `true` = 本窗**含该次派发的末片**（或收尾合并窗）⇒ `committed_len` 已覆盖整次派发；
        /// `false` = **部分窗**（一次多片派发被延后末片前的中间窗）⇒ `committed_len` 按片样本占比
        /// 折算，**不得**用 `ReflowCommit` 的整次派发边界（否则会截掉被延后末片那段流式文字）。
        boundary_usable: bool,
        has_hole: bool,
        acc_text: String,
        /// SLIDING-WINDOW-367：`true` ⇒ 消费端**整段替换**预览为 `acc_text`（滑窗权威全文，
        /// 非前缀关系，不能按 `committed_len` 切；Gavin 方案「整体替换」）；`false` ⇒ 沿用 325
        /// 原语义（替换前 `committed_len` 字符、保留流式尾巴）。**显式声明节点行为（DEC-066）**，
        /// 不靠长度关系反推。
        replace_all: bool,
        /// FIX-WINDOW-COVER-AND-EARLY-PROCESSING-382（3C）：本窗**解码完成**时刻
        /// （worker `Instant::now()`，随解码结果一路带到事件）。消费端在**实际 `show_overlay` 渲染**时
        /// 打 `decode_done→render_ms`（端到端口径：解完 → 浮层重画）。`None` = 老发送方 / 非滑窗路径。
        decode_done_at: Option<std::time::Instant>,
        /// LOCALRT-PREVIEW-HIDE-NONUSER-429：`true` ⇒ 本窗被声纹**整窗全剔**（421 full_drop），
        /// 预览合成 = **已定稿权威文本** + `streaming[committed_len..]`（该段之后的流式尾巴），
        /// **不含该段对应的流式文字**；`acc_text` 为空时也按此合成（不被 `compose_with_acc_for_gen`
        /// 的「acc 空 ⇒ 原样 raw」规则短路）。其余场景恒 `false`（逐位不变）。
        non_user_hide: bool,
        /// ACC-ENGINE-LLAMACPP-452 ⑤：`true` ⇒ **精解半截结果**（流式回灌）。`committed_len` 为本窗替换边界上限；
        /// 消费端按 `partial_reflow_boundary` 现算「精解写到原始预览的哪个位置」作本次替换边界，
        /// 显示 = 半截精解 + 预览中尚未解到的部分（不回缩）；不走整窗回灌的边界配对。
        partial: bool,
        /// ASR-SERVER-491 S9：与 `acc_text` 逐字对应的时间（**虚拟时间轴**秒，见 `char_times::SliceMap`）。
        /// 只有远程会话填（本机恒 `None`，浮层不读）。
        acc_times: Option<Vec<Option<transcription::char_times::CharTime>>>,
    },
    /// LOCALRT-SEAM-337（自适应定界）：本片边界冻结通知。
    ///
    /// `committed_len = Some(n)`（分支 a 文本停止增长 / c 硬上限）⇒ 用该片 pending `acc_text`
    /// 覆盖流式显示前 n 字符并渲染；`None`（分支 b 有声恢复）⇒ 该片**不回灌**，保持纯流式。
    /// 只有本地实时档会发（在线/批处理结构上不发）。
    ReflowCommit {
        generation: u64,
        seg_index: usize,
        committed_len: Option<usize>,
    },
}

/// FIX-PREVIEW-STALE-AND-COLLAPSE-375（A）：`replace_all=true` 路径**专用**的单调键 = 滑窗快照的
/// `reflow_seq`（`seg_index` 在滑窗路径会重复，见事件字段注释）。两条路径各用一个计数器，
/// 互不干扰 ⇒ 老逐片路径行为逐位不变。
#[cfg(target_os = "windows")]
static ACC_REFLOW_LAST_REFLOW_SEQ: AtomicI64 = AtomicI64::new(-1);

// ACC-REFLOW-PERSIST-329：本代已确定的**权威前缀**状态 `(generation, acc_text, committed_len)`。
// - 只在 `PreviewReflow`(Applied) 时写（唯一发送点 = 本地实时档）⇒ 在线/批处理恒 None；
// - `RecordingStarted` 清空；流式渲染时按 gen 匹配取用；
// - **闩锁之后保留**（只停止更新，不丢弃）⇒ 画面不倒退。
// - controller 单线程读写；`Mutex` 仅因是 `static`（同 `last_streaming_text` 先例）。
#[cfg(target_os = "windows")]
static ACC_REFLOW_STATE: Mutex<Option<(u64, String, usize)>> = Mutex::new(None);

/// FIX-WINDOW-COVER-AND-EARLY-PROCESSING-382（3A）：`replace_all` 回灌的快速路径状态机
///（最新全文 / `(gen,seg)` 边界小 map / 最新已渲染 seg）。按代清空。
#[cfg(target_os = "windows")]
static ACC_REFLOW_FAST: Mutex<ReflowFastState> = Mutex::new(ReflowFastState {
    latest: None,
    bounds: Vec::new(),
    rendered: None,
});

/// FIX-WINDOW-COVER-AND-EARLY-PROCESSING-382（问题2 防闪回）：本代浮层**已进入处理态**。
/// controller **处理到本代 `Processing` 事件时**由 controller 线程置位（按通道实际顺序：排在
/// Processing 之前入队的回灌照常渲染，之后的只更新状态、不重画）⇒ 提前发 Processing 后，acc
/// 尾窗的迟到回灌不会把浮层拉回预览态。`RecordingStarted` 复位。只作用于本地实时档
///（只有它会发 `PreviewReflow`）。
#[cfg(target_os = "windows")]
static ACC_REFLOW_SUPPRESS: AtomicBool = AtomicBool::new(false);

fn clone_runtime_config(shared_config: &Arc<RwLock<AppConfig>>) -> AppConfig {
    match shared_config.read() {
        Ok(cfg) => cfg.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

/// 内部错误串 → 浮层提示文案（三语 i18n）。
///
/// 10-14 Gavin「UI和提示对话框中的文字一定都要走i18n国际化」：内部错误串有中有英，
/// 原样上屏会让英文界面冒中文。中文关键词与英文同表归类；**未归类一律显示
/// `error_unexpected`，不再原文直出**（调用方发 Error 前均已 log::error 记下原文）。
/// 🔴 「没说话」不在此嗅探（H3 护栏）：它走 NoSpeechError 类型化信号，不进本函数。
fn convert_to_friendly_error(message: &str, ui_language: config::UiLanguage) -> String {
    let strings = i18n::get(ui_language);
    let m = message.to_lowercase();
    let has = |keys: &[&str]| keys.iter().any(|k| m.contains(k));
    // ASR-SERVER-491 S8：自家识别服务的错误码（设计 §5.1）先认，否则会被下面的「服务端」归成接口不可用。
    if has(&["modelnotready"]) {
        return strings.error_feiyin_not_ready.to_string();
    }
    if has(&["servicebusy", "servicepaused"]) {
        return strings.error_feiyin_busy.to_string();
    }
    if has(&[
        "invalidkey",
        "keyrevoked",
        "keyexpired",
        "devicemismatch",
        "fingerprintmismatch",
        "notpaired",
    ]) {
        return strings.error_feiyin_repair.to_string();
    }
    if has(&["timeout", "timed out", "超时"]) {
        strings.error_network_timeout.to_string()
    } else if has(&[
        "api",
        "http",
        "401",
        "403",
        "网络",
        "服务端",
        "鉴权",
        "连接",
        "tls",
    ]) {
        strings.error_api_unavailable.to_string()
    } else if has(&["model", "recognizer", "transcriber", "模型"]) {
        strings.error_model_init.to_string()
    } else if has(&["audio", "microphone", "麦克风", "音频", "录音"]) {
        strings.error_microphone.to_string()
    } else if has(&["mic_muted"]) {
        strings.error_mic_muted.to_string()
    } else {
        strings.error_unexpected.to_string()
    }
}

/// VAD-393（A3）：把「窗内各片的片内语音区间」按各片样本数偏移，拼成**窗内坐标**的合并区间表。
///
/// `slices[k]` 为第 k 片的 `Option<区间表>`（`None` = 该片无实时时间线）；`lens[k]` 为第 k 片样本数。
/// 任一源片为 `None` ⇒ 返回 `None`（整窗回退自跑 VAD）；全为 `Some` ⇒ 拼接（不做合并，语义上
/// 各片区间本就不重叠）。纯函数，可单测。
fn shift_and_concat_ranges(
    slices: &[Option<Vec<(usize, usize)>>],
    lens: &[usize],
) -> Option<Vec<(usize, usize)>> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut offset = 0usize;
    for (i, s) in slices.iter().enumerate() {
        let ranges = s.as_ref()?;
        for &(a, b) in ranges {
            out.push((a + offset, b + offset));
        }
        offset += lens.get(i).copied().unwrap_or(0);
    }
    Some(out)
}

/// PARALLEL-ACC-298：把 accuracy 并行各片的子段文本按 `seg_index` **升序**拼接成扁平列表。
///
/// 单 worker + channel FIFO 已保证顺序，这里仍显式排序（乱序/缺号输入也稳定），
/// 是判据 #3「seg_index 有序拼接」的纯函数载体（可单测，无需真模型）。
// SLIDING-WINDOW-367：单窗口解码（窗口音频 + 注入集中一处）。
fn decode_window(
    recognizer: &transcription::AccEngine,
    window_audio: &[f32],
    window_seq: usize,
    script: config::ChineseScript,
    terms: Option<&str>,
    // FIX-PREVIEW-STALE-AND-COLLAPSE-375（B）：本次录音已定稿窗口的产出率均值快照（冷启动 None）。
    avg_chars_per_sec: Option<f32>,
    // VAD-393（A3）：本窗**窗内坐标**的语音区间（实时 VAD 时间线映射而来）；`None` ⇒ 回退自跑 VAD。
    speech_ranges: Option<&[(usize, usize)]>,
    // VAD-393（A4）：本窗流式文本是否非空（区间为空串时决定「整窗解码 vs 提前返回」）。
    streaming_nonempty: bool,
    // SPEAKER-VERIFY-408B：本窗新片起点样本下标（声纹注册只计新片）。
    new_slice_from: usize,
    // ACC-452：本窗解码提示（② 预览草稿）。
    hints: &AccHints,
    // ACC-452 ⑤：逐字回调（流式回灌；`None` = 不回调）。
    on_partial: Option<&(dyn Fn(&str) + Sync)>,
) -> anyhow::Result<(String, bool, transcription::AccDropStats)> {
    transcription::transcribe_acc_ctx(
        recognizer,
        window_audio,
        script,
        window_seq,
        transcription::CtxInject {
            terms,
            avg_chars_per_sec,
            speech_ranges,
            streaming_nonempty,
            new_slice_from,
            assist: transcription::llama_asr::DecodeAssist {
                draft: (!hints.draft.is_empty()).then_some(hints.draft.as_str()),
                on_partial,
            },
        },
    )
}

/// VOICEPRINT-LEAK-460：本次录音是否「声纹把语音全剔」——没有任何保留的本人语音、且确有被剔除的语音。
/// 为真且滑窗结果为空 ⇒ 不走整段兜底解码（兜底不经声纹）。
fn voiceprint_dropped_all(kept_secs: f32, dropped_secs: f32) -> bool {
    kept_secs <= 0.0 && dropped_secs > 0.0
}

/// PUNCT-DOUBLE-334：本地并行 accuracy 文本的 `native_punctuated` 判定。
///
/// DEC-047 口径 = **文本实测**：`punctuation::has_effective_punctuation`（词内嵌标点豁免，
/// `3.14`/`3:30`/`example.com` 不算真标点），与 `main.rs` 的 Qwen3 `initial_text` 分支同一 detector。
///
/// 🔴 为什么不能用「各分片是否都解码成功」（原 `acc_all_native` 的语义）：分片失败 ≠ 文本无标点。
/// 任一片 Err ⇒ 旧值 false ⇒ `apply_local_punctuation` 的 `!native_punctuated` 门通过 ⇒
/// CT-Transformer 对**已带标点**的全文再打一遍 ⇒ 位置重合处 `。。`、不重合处各占一个（叠加指纹）。
///
/// 已知取舍（**记录在案，不得当 bug 重修**）：
/// 1. 带洞（失败片留空串）且其余已带标点 ⇒ 判 true ⇒ 跳过引擎 ⇒ 洞那段不补标点。
///    可接受：洞（缺整段文字）是更大缺陷；**为补洞而补跑会把旁边已打标点的好段二次打点**
///    （正是本单要消除的 `。。` 叠加）；且预览侧 `SkippedHole`（325/329）已拦住带洞回灌。
/// 2. `has_effective_punctuation` 口径是「任一无 ASCII 夹持的标点出现即 true」，**不要求句末标点**：
///    文本只有一个逗号、没有句号 ⇒ true ⇒ 跳过引擎 ⇒ **最终输出没有句号**。
///    这**不是新缺陷**，是 DEC-047 白纸黑字记录过的「已知接受边界」
///    （`decisions-archive.md:1174` 表格：「`他说“好”`（中文引号，无句号）→ 本决策口径 true →
///    跳过标点引擎 → **接受** —— 引号无句号不是用户会报的缺陷」，该表开头即「不得当 bug 重修」）。
///    🔴 放宽此判据会直接把重复标点带回来 ⇒ 见到「少个句号」**不要动判据**。
fn pretranscribed_native_punctuated(text: &str) -> bool {
    punctuation::has_effective_punctuation(text)
}

/// ACC-PREVIEW-REFLOW-325：合成回灌预览 —— `acc_text` 替换 `streaming` 的前 `committed_len`
/// 个**字符**，其余保留流式尾巴。按**字符**（非字节）切，避免 UTF-8 切裂。
fn reflow_preview(acc_text: &str, streaming: &str, committed_len: usize) -> String {
    let mut out = String::with_capacity(acc_text.len() + streaming.len());
    out.push_str(acc_text);
    out.extend(streaming.chars().skip(committed_len));
    out
}

/// ACC-ENGINE-LLAMACPP-452 ⑤：流式回灌的替换边界（纯函数）—— 精解半截结果「写到了原始预览的哪个位置」。
///
/// - `prev_acc` / `prev_len`：当前已生效的权威文本及其在原始预览中的边界（上次回灌）；
/// - `candidate`：试拼出的半截全文；`raw`：原始预览；`window_end`：本窗替换边界上限（整窗回灌的边界）。
///
/// 做法：从 `prev_len` 起按「半截新增有效字数」在 `raw` 中往后数得估计位置；再用 `candidate` 末尾两个有效字
/// 在估计位置 ±[`PARTIAL_BOUNDARY_SLACK`] 字内对齐校准（取最近者）。结果须在 `(prev_len, window_end]` 内，
/// 否则 `None`（本次不刷新）。显示 = `candidate` + `raw[边界..]` ⇒ 预览里尚未解到的字保留，不回缩。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))] // 消费端（controller 臂）仅 Windows
fn partial_reflow_boundary(
    prev_acc: &str,
    prev_len: usize,
    candidate: &str,
    raw: &str,
    window_end: usize,
) -> Option<usize> {
    let is_eff = |c: &char| c.is_alphanumeric();
    let raw: Vec<char> = raw.chars().collect();
    let end = window_end.min(raw.len());
    let start = prev_len.min(end);
    let cand_eff: Vec<char> = candidate.chars().filter(is_eff).collect();
    let prev_eff = prev_acc.chars().filter(is_eff).count();
    let n_new = cand_eff.len().checked_sub(prev_eff).filter(|&n| n > 0)?;
    // 估计位置：从 start 起数 n_new 个有效字。
    let (mut p, mut seen) = (start, 0usize);
    while p < end && seen < n_new {
        if is_eff(&raw[p]) {
            seen += 1;
        }
        p += 1;
    }
    // 校准：末尾两个有效字在估计位置附近的最近匹配（匹配终点 = 第二个字之后）。
    if cand_eff.len() >= 2 {
        let (a, b) = (cand_eff[cand_eff.len() - 2], cand_eff[cand_eff.len() - 1]);
        let eff_idx: Vec<usize> = (start..end).filter(|&i| is_eff(&raw[i])).collect();
        let best = eff_idx
            .windows(2)
            .filter(|w| raw[w[0]] == a && raw[w[1]] == b)
            .map(|w| w[1] + 1)
            .filter(|&q| q.abs_diff(p) <= PARTIAL_BOUNDARY_SLACK)
            .min_by_key(|&q| q.abs_diff(p));
        if let Some(q) = best {
            p = q;
        }
    }
    (p > start && p <= end).then_some(p)
}

/// ACC-452 ⑤：流式回灌允许的显示缩短（字）—— 精解去掉预览里的口水词属正常修正；超过即跳过本次刷新。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const PARTIAL_SHRINK_TOLERANCE: usize = 2;

/// ACC-452 ⑤：替换边界校准的搜索半径（字）。
const PARTIAL_BOUNDARY_SLACK: usize = 4;

/// ACC-REFLOW-PERSIST-329：流式渲染/镜像前的权威前缀合成。
///
/// `state` = 本代已确定的 `(generation, acc_text, committed_len)`（见 [`ACC_REFLOW_STATE`]）。
/// - gen 匹配 **且** `acc_text` 非空 ⇒ `reflow_preview(acc, raw, committed)`（权威前缀 + 流式尾巴）；
/// - 其余（state 为 None / gen 不匹配 / acc 空）⇒ **原样返回 `raw`**
///   ⇒ 在线 / 批处理结构上恒走此分支，行为逐位不变。
///
/// 🔴 **滚动分界线口径（Gavin 2026-09-21 明确，勿改）**：
/// `preview = accuracy 权威前缀（已完成分片） + 流式尾巴（正在说的那段）`。
/// 正在说的那段 accuracy 还没拿到（要攒够 `min_seg_ms` 且静默 800ms 才派片），
/// 所以**流式尾巴在每一刻都存在**，不是「第一句才用流式」的特例；分界线随分片完成不断右移。
/// BUILD-321 实测：第一片在 ~5.65s 派发、解码 1.2~1.9s ⇒ **前 6~8 秒全是流式文字**，
/// 之后才有回灌。⇒ **`acc_text` 为空时必须原样返回流式全文**（否则开口后前 6~8 秒会是空白，
/// 属致命回归；见 `compose_empty_acc_passthrough` 单测锁定）。
fn compose_with_acc_for_gen(
    state: Option<&(u64, String, usize)>,
    raw_gen: u64,
    raw: &str,
) -> String {
    match state {
        Some((g, acc, committed)) if *g == raw_gen && !acc.is_empty() => {
            reflow_preview(acc, raw, *committed)
        }
        _ => raw.to_string(),
    }
}

/// FIX-TAIL-WINDOW-AND-FALLBACK-386（B）：回灌渲染与后续 `StreamingText` 渲染用**同一个合成函数**
///（acc 全文 + `streaming[committed_len..]` 流式尾巴）。
///
/// 🔴 不得再走已删除的 `reflow_preview_367`：它对 `replace_all` 只返回 `acc_text`、**丢掉流式尾巴** ⇒
/// 说话中途回灌会把预览截短（实测 31→28、78→69、191→146），下一次 `StreamingText` 才拼回 ⇒ 闪回。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn compose_reflow_preview(
    generation: u64,
    acc_text: &str,
    streaming: &str,
    committed_len: usize,
) -> String {
    let state = (generation, acc_text.to_string(), committed_len);
    compose_with_acc_for_gen(Some(&state), generation, streaming)
}

/// 382：只更新权威状态（`ACC_REFLOW_STATE`），**不重画浮层**。用于「本代 Processing 已开始 ⇒
/// 回灌只更新状态、不重画」的抑制路径（防闪回）。
#[cfg(target_os = "windows")]
fn set_acc_reflow_state_only(generation: u64, acc_text: &str, committed_len: usize) {
    if let Ok(mut st) = ACC_REFLOW_STATE.lock() {
        *st = Some((generation, acc_text.to_string(), committed_len));
    }
}

/// LOCALRT-RELEASE-REFLOW-342-A：停止语义**三态**（消费端 controller 线程判定）。
///
/// 329 曾把 `STREAMING_STOPPED` 一刀切当「取消」⇒ 把**松手完成**与**取消**混为一谈。
/// 该 latch 实际被三种情形置位，语义完全不同：
/// - **取消**（ESC / PTT < 300ms，`cancel_signal`）⇒ 内容丢弃，**不回灌**；
/// - **编辑**（`OVERLAY_EDITING`，进编辑时也置 `STREAMING_STOPPED`）⇒ 跳过，保护用户文本；
/// - **松手完成**（正常 PTT 松手 / 仍在录音）⇒ 内容要用（最终文本本就走 accuracy），
///   **允许回灌刷新预览**。
///
/// 🔴 判据用 `cancel_signal`（取消专用信号），**不再用 `STREAMING_STOPPED` 一刀切**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReflowStopState {
    /// ESC / 短按取消：丢弃，不回灌。
    Cancelled,
    /// 编辑中（本代编辑闩锁另由 `edit_latched` 持久）。
    Editing,
    /// 松手完成 / 仍在录音：都允许回灌。
    Released,
}

fn reflow_stop_state(cancelled: bool, editing: bool) -> ReflowStopState {
    if cancelled {
        ReflowStopState::Cancelled
    } else if editing {
        ReflowStopState::Editing
    } else {
        ReflowStopState::Released
    }
}

/// ACC-PREVIEW-REFLOW-325：回灌决策（纯函数，消费端 controller 线程调用）。
///
/// 优先级（高 → 低）：
/// 1. **编辑**（本代编辑闩锁 `edit_latched`，或当前正在编辑 `stop_state=Editing`）——
///    防「编辑退出后迟到的回灌把用户刚打的字冲掉」；
/// 2. **取消**（`stop_state=Cancelled`，仅 ESC / 短按）—— 松手完成**不**在此列（342-A）；
/// 3. **陈旧 seg**（`seg_index <= 已应用`）—— 单调键是 seg_index，不是 committed_len；
/// 4. **覆盖区有洞**（该片覆盖范围内含 accuracy 失败片）—— 带洞文本比空串更糟；
/// 5. **acc 空**（不能把已显示前缀抹白）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReflowAction {
    Applied,
    SkippedEditing,
    SkippedCancel,
    SkippedStale,
    SkippedHole,
    SkippedEmpty,
}

/// FIX-PREVIEW-STALE-AND-COLLAPSE-375（A）：回灌「过期」判据的**单调键**选择。
///
/// - `replace_all=true`（滑窗权威全文，语义 = 整段替换、每条取代前一条）⇒ 用**滑窗快照自有**的
///   `reflow_seq`（严格递增）。🔴 **不得**用 `seg_index`：滑窗路径传的是切片下标，窗口间
///   （滑窗调度 + 收尾 drain）会让多个窗口带**同一个下标** ⇒ 后到的那个
///   （= `OrderedReflow` 合并后的完整文本）被误判 stale 丢掉 ⇒ 预览永远停在纯流式文本。
/// - `replace_all=false`（老逐片路径）⇒ 仍用 `seg_index`（逐片严格递增），**行为逐位不变**。
///
/// `reflow_seq` 缺失（老发送方/异常）⇒ 退回 `seg_index`（保守：沿用旧行为，不至于整片不回灌）。
fn reflow_monotonic_key(replace_all: bool, seg_index: usize, reflow_seq: Option<usize>) -> i64 {
    if replace_all {
        reflow_seq.unwrap_or(seg_index) as i64
    } else {
        seg_index as i64
    }
}

fn reflow_action(
    edit_latched: bool,
    stop_state: ReflowStopState,
    key: i64,
    last_applied_key: i64,
    has_hole: bool,
    acc_empty: bool,
) -> ReflowAction {
    if edit_latched || stop_state == ReflowStopState::Editing {
        return ReflowAction::SkippedEditing;
    }
    if stop_state == ReflowStopState::Cancelled {
        return ReflowAction::SkippedCancel;
    }
    if key <= last_applied_key {
        return ReflowAction::SkippedStale;
    }
    if has_hole {
        return ReflowAction::SkippedHole;
    }
    if acc_empty {
        return ReflowAction::SkippedEmpty;
    }
    ReflowAction::Applied
}

/// FIX-PREVIEW-HARVEST-380（A）：accuracy 滑窗线程的切片载荷
/// `(切片下标, 派发当刻浮层字符数, 本片音频, 本片流式文本, 各片内语音区间)`。
/// VAD-393（A3）：第 5 元为**片内坐标**的语音区间（实时 VAD 时间线映射）；VAD 不可用 ⇒ `None`。
type AccSliceMsg = (
    usize,
    usize,
    Vec<Vec<f32>>,
    String,
    Option<Vec<Vec<(usize, usize)>>>,
    // FALLBACK-EXCLUDE-FARFIELD-446：各子片片内坐标的远场区间（只供流式兜底剔除）。
    Vec<Vec<(usize, usize)>>,
);

/// LOCALRT-TAIL-WINDOW-407：滑窗线程的**输入**（新切片 / 长静默信号）。
///
/// 380 起 `acc_tx` 只传切片元组；407 追加「长静默」信号（本地流式在静默 ≥1900ms 且无新语音时
/// **只发一次**），main 侧据此把「待覆盖片 + 前一片后缀」组末尾窗并即时回灌，**无需按停止键**。
/// `drive_acc_windows` 仍是泛型（380/406 守卫与单测不受影响）。
enum AccInput {
    Slice(AccSliceMsg),
    /// 长静默信号，载荷 = 当刻 `pcm` 样本位置（仅日志/诊断用）。
    LongSilence(usize),
}

/// FIX-TAIL-GUARD-410：末尾窗的「pending 片」回灌元数据。
///
/// 末尾窗音频含前片后缀 + pending；若精解被 406 拒 / 为空，**只兜底 pending 片**
///（span `(p,p+1)`、文本 = pending 片流式 + 标点），**前片已回灌结果一字不动**。
struct TailPending {
    span: (usize, usize),
    samples: Vec<usize>,
    streaming: String,
}

/// FIX-TAIL-GUARD-410 item3：兜底打点的**同步超时**（ms）。超时 ⇒ 原样用未打标点文本，绝不卡回灌。
const PUNCT_FALLBACK_TIMEOUT_MS: u64 = 500;

/// 410：服务线程**异常退出**后重建 + 重发，最多连续重试次数。
const PUNCT_START_MAX_RETRIES: usize = 2;

/// 410：连续启动失败后的**冷却**（秒）——冷却期内不再尝试启动，防模型缺失时每段都反复加载拖慢回灌。
const PUNCT_RESTART_COOLDOWN_SECS: u64 = 30;

/// 410 item3：标点服务线程的请求（文本 + 回执通道；回执 `None` = 引擎不可用）。
struct PunctReq {
    text: String,
    reply: crossbeam_channel::Sender<Option<String>>,
}

/// 410 item3：**进程级常驻**标点服务状态。
#[derive(Default)]
struct PunctState {
    /// 服务请求发送端（`None` = 未启动 / 已异常退出）。
    tx: Option<crossbeam_channel::Sender<PunctReq>>,
    /// 服务线程句柄（`is_finished()` ⇒ 已退出，可据此在发送前判异常退出）。
    join: Option<std::thread::JoinHandle<()>>,
    /// 上次「启动/重试全失败」时刻（冷却用）。
    last_fail: Option<std::time::Instant>,
}

static PUNCT_SERVICE: std::sync::OnceLock<std::sync::Mutex<PunctState>> =
    std::sync::OnceLock::new();

/// 单次调用的结果分类。
enum PunctTry {
    /// 成功打点。
    Ok(String),
    /// 引擎不可用（模型缺失 / 加载失败）——属启动失败，计入重试/冷却。
    Unavailable,
    /// 线程已退出 / panic（send / recv 断开）——需重启。
    AbnormalExit,
    /// 线程活着但处理太慢 —— **不算异常退出、不重启**。
    Timeout,
}

/// 410：冷却是否生效（距上次失败 < [`PUNCT_RESTART_COOLDOWN_SECS`] ⇒ 本次不尝试启动）。纯函数。
fn punct_cooldown_active(last_fail: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    last_fail.is_some_and(|t| {
        now.saturating_duration_since(t)
            < std::time::Duration::from_secs(PUNCT_RESTART_COOLDOWN_SECS)
    })
}

/// 410：调用→重试循环（与 I/O 解耦，便于单测「异常退出后重启成功 / 两次失败走无标点 / 超时不重启」）。
///
/// - `Ok(s)` ⇒ 成功返回；
/// - `Timeout` ⇒ 立即 `Err("timeout")`（**不重试**，④）；
/// - `AbnormalExit` / `Unavailable` ⇒ **重启重试**，最多 [`PUNCT_START_MAX_RETRIES`] 次，全失败 ⇒ `Err("startup-failed")`。
fn run_punct_retries<F: FnMut(usize) -> PunctTry>(mut attempt: F) -> Result<String, &'static str> {
    for i in 0..=PUNCT_START_MAX_RETRIES {
        match attempt(i) {
            PunctTry::Ok(s) => return Ok(s),
            PunctTry::Timeout => return Err("timeout"),
            PunctTry::AbnormalExit | PunctTry::Unavailable => {}
        }
    }
    Err("startup-failed")
}

/// 410：确保服务线程在跑（已退出 ⇒ 重启；引擎在**服务线程内**加载 ⇒ 不要求 `PunctuationEngine: Send`）。
fn ensure_punct_service(state: &std::sync::Mutex<PunctState>, model_dir: &Path) {
    let mut g = state.lock().unwrap_or_else(|e| e.into_inner());
    // 发送前判异常退出：线程句柄已完成 ⇒ 清掉发送端，下面重建。
    if g.join.as_ref().is_some_and(|h| h.is_finished()) {
        g.tx = None;
        g.join = None;
    }
    if g.tx.is_some() {
        return;
    }
    let (tx, rx) = crossbeam_channel::unbounded::<PunctReq>();
    let dir = model_dir.to_path_buf();
    let spawned = std::thread::Builder::new()
        .name("punct-fallback-service".to_string())
        .spawn(move || {
            let t0 = std::time::Instant::now();
            let mut engine = punctuation::PunctuationEngine::new(&dir);
            log::info!(
                "[LocalRT-DBG-410] punct fallback service: model load={:.1}ms status={}",
                t0.elapsed().as_secs_f64() * 1000.0,
                if engine.is_some() {
                    "ok"
                } else {
                    "unavailable"
                }
            );
            for req in rx {
                let out = engine.as_mut().and_then(|e| {
                    let t = std::time::Instant::now();
                    let r = e.add_punctuation(&req.text);
                    if log::log_enabled!(log::Level::Debug) {
                        log::debug!(
                            "[LocalRT-DBG-410] punct service call: {} chars in {:.1}ms ok={}",
                            req.text.chars().count(),
                            t.elapsed().as_secs_f64() * 1000.0,
                            r.is_some()
                        );
                    }
                    r
                });
                let _ = req.reply.send(out);
            }
        });
    match spawned {
        Ok(h) => {
            g.tx = Some(tx);
            g.join = Some(h);
        }
        Err(_) => {
            log::warn!("[LocalRT-DBG-410] punct fallback service spawn failed");
            g.tx = None;
            g.join = None;
        }
    }
}

/// 410：单次尝试（确保服务在跑 → 发送 → 同步收结果）。
fn punct_attempt(state: &std::sync::Mutex<PunctState>, model_dir: &Path, text: &str) -> PunctTry {
    ensure_punct_service(state, model_dir);
    let tx = state.lock().unwrap_or_else(|e| e.into_inner()).tx.clone();
    let Some(tx) = tx else {
        return PunctTry::Unavailable; // 启动失败
    };
    let (reply_tx, reply_rx) = crossbeam_channel::bounded::<Option<String>>(1);
    let req = PunctReq {
        text: text.to_string(),
        reply: reply_tx,
    };
    if tx.send(req).is_err() {
        // 线程已退出（含 panic）⇒ 标记，下一轮 ensure 重建。
        let mut g = state.lock().unwrap_or_else(|e| e.into_inner());
        g.tx = None;
        g.join = None;
        return PunctTry::AbnormalExit;
    }
    match reply_rx.recv_timeout(std::time::Duration::from_millis(PUNCT_FALLBACK_TIMEOUT_MS)) {
        Ok(Some(s)) => PunctTry::Ok(s),
        Ok(None) => PunctTry::Unavailable, // 引擎加载失败（线程活着但引擎 None）
        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
            // 线程在本次请求中 panic ⇒ 标记，下一轮重建。
            let mut g = state.lock().unwrap_or_else(|e| e.into_inner());
            g.tx = None;
            g.join = None;
            PunctTry::AbnormalExit
        }
        Err(crossbeam_channel::RecvTimeoutError::Timeout) => PunctTry::Timeout,
    }
}

/// 410 item3：经进程级标点服务线程打点（同步、超时、异常退出自动重启，最多 2 次）。
///
/// `enabled=false` / 已含句读标点 / 冷却期内 / 重试全失败 / 超时 ⇒ 原样返回（并 warn），**绝不阻塞回灌**。
fn punctuate_via_service(model_dir: &Path, enabled: bool, text: &str) -> String {
    if !enabled || text.trim().is_empty() {
        return text.to_string();
    }
    // FIX-VOICEPRINT-FALLBACK-421：不再「含任一标点就整段跳过」（流式里只要有 1 个停顿打的句号，
    // 整段都不打点 ⇒ Gavin 端测「最后一句没有标点」）。改为先**剥掉**流式里零星的标点，再整段送
    // CT-Transformer（与预览「输入始终是原始裸文本」同原则）。**失败/超时仍原样返回原文**（不丢标点）。
    let stripped = strip_punct_for_repunct(text);
    if stripped.trim().is_empty() {
        return text.to_string();
    }
    let state = PUNCT_SERVICE.get_or_init(|| std::sync::Mutex::new(PunctState::default()));
    let now = std::time::Instant::now();
    {
        let g = state.lock().unwrap_or_else(|e| e.into_inner());
        if punct_cooldown_active(g.last_fail, now) {
            log::warn!("[LocalRT-DBG-410] punct service in cooldown; using raw fallback text");
            return text.to_string();
        }
    }
    match run_punct_retries(|_i| punct_attempt(state, model_dir, &stripped)) {
        Ok(s) => {
            state.lock().unwrap_or_else(|e| e.into_inner()).last_fail = None;
            if s.trim().is_empty() {
                text.to_string()
            } else {
                s
            }
        }
        Err(why) => {
            if why == "timeout" {
                log::warn!("[LocalRT-DBG-410] punct fallback timeout; using raw fallback text");
            } else {
                // 启动/重试全失败 ⇒ 设冷却（③：下次仍需重试，但 30s 内不反复加载）。
                state.lock().unwrap_or_else(|e| e.into_inner()).last_fail = Some(now);
                log::warn!(
                    "[LocalRT-DBG-410] punct service failed after retries ({why}); using raw fallback text"
                );
            }
            text.to_string()
        }
    }
}

/// FIX-VOICEPRINT-FALLBACK-421：剥掉标点供**重新整段打点**（含中英标点）。数字中的小数点 / 千分
/// 逗号（两侧皆 ASCII 数字）保留，防 `3.14` → `314`。
fn strip_punct_for_repunct(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        let is_punct = matches!(
            c,
            '，' | '。' | '、' | '！' | '？' | '；' | '：' | ',' | '.' | '!' | '?' | ';' | ':'
        );
        if !is_punct {
            out.push(c);
            continue;
        }
        if matches!(c, '.' | ',') {
            let prev_digit = i > 0 && chars[i - 1].is_ascii_digit();
            let next_digit = i + 1 < chars.len() && chars[i + 1].is_ascii_digit();
            if prev_digit && next_digit {
                out.push(c); // 数字内部的小数点 / 千分逗号保留。
            }
        }
    }
    out
}

/// FIX-PREVIEW-HARVEST-380（A）：滑窗解码结果载荷
/// `(window_seq, dispatch_idx, 解码结果, 解码耗时 ms, 解码完成时刻)`。
/// 382（3C）：末元 `Instant` 随 `PreviewReflow.decode_done_at` 带到消费端，测「解完 → 浮层重画」。
type AccDecodeResult = (
    usize,
    usize,
    anyhow::Result<(String, bool, transcription::AccDropStats)>,
    f64,
    std::time::Instant,
);

/// PIPE-SPEED-457 ①：解码线程 → 收割线程的消息。**同一通道**：同一线程先发的先到 ⇒
/// 第 N 窗的对齐时间必在第 N+1 窗精解结果之前被处理（重拼只针对「最近拼接的窗」）。
enum AccWorkerMsg {
    /// 精解结果（先送，不等对齐）。
    Decoded(AccDecodeResult),
    /// FORCED-ALIGN-456：该窗精解文字每字（起, 止）**窗内秒**（强制对齐，晚到）。
    Aligned(usize, Vec<(f32, f32)>),
}

/// FIX-WINDOW-COVER-AND-EARLY-PROCESSING-382：滑窗解码任务载荷
/// `(window_seq, dispatch_idx, 本窗音频, 产出率均值快照, 派发时刻, 本窗语音区间, 本窗流式文本非空, 本窗新片起点样本下标)`
/// （派发时刻供 3C `queued_ms`）。VAD-393（A3）：第 6 元为**窗内坐标**的合并语音区间；
/// `None` ⇒ 回退 391 自跑 VAD。SPEAKER-VERIFY-408B：第 8 元 `new_slice_from`（声纹注册只计新片）。
type AccTaskMsg = (
    usize,
    usize,
    Vec<f32>,
    Option<f32>,
    std::time::Instant,
    Option<Vec<(usize, usize)>>,
    bool,
    usize,
    AccHints,
);

/// ACC-452 ⑤：流式回灌的半截结果最短发送间隔（每字都发会刷屏；~6 字 / 间隔）。
const PARTIAL_REFLOW_INTERVAL: std::time::Duration = std::time::Duration::from_millis(150);

/// ACC-ENGINE-LLAMACPP-452：本窗解码提示（随任务派发）。上文注入经 452 回放未采用（加在词库之上 CER 4.63%→4.80%）。
#[derive(Default, Clone)]
struct AccHints {
    /// ② 本窗预览（流式）文字 ⇒ 推测解码草稿（只省时间，不改输出）。
    draft: String,
}

/// FIX-PREVIEW-HARVEST-380（A）：滑窗线程的一步（新切片 / 解码结果）。
enum AccWindowStep<A, R, P> {
    Slice(A),
    Result(R),
    /// ACC-452 ⑤：解码中的半截结果（流式回灌）。
    Partial(P),
}

/// FIX-PREVIEW-HARVEST-380（A）：滑窗线程「结果到即收」驱动器。
///
/// 同时等 `acc_rx`（新切片）与 `res_rx`（解码结果），**任一先到即处理**；`acc_rx` 关闭
///（松键）或 `cancelled()` 为真即返回，由调用方做收尾 drain（复用同一 `step` 闭包）。
///
/// 缺陷本体（Gavin 2026-09-23 端测 BUILD-379）：原实现是 `for … in acc_rx` —— 没有新切片时
/// 不进循环体，已解出的结果要压到下一次切片或松键才回灌（用户停顿 ⇒ 预览不刷新，每次的
/// 最后一窗都要等松键）。抽成独立函数的唯一目的，是让「结果在 `acc_rx` 仍打开时就被处理」
/// 这条不变量**可对生产代码单测**（见 `preview_harvest_380_tests`），而不是测一份复制品。
fn drive_acc_windows<A, R, P>(
    acc_rx: &crossbeam_channel::Receiver<A>,
    res_rx: &crossbeam_channel::Receiver<R>,
    partial_rx: &crossbeam_channel::Receiver<P>,
    cancelled: impl Fn() -> bool,
    step: &mut impl FnMut(AccWindowStep<A, R, P>),
) {
    // ACC-452 ⑤：半截结果通道断开（不应发生）⇒ 换成永不就绪的通道，避免 select 空转。
    let mut partial_rx = partial_rx.clone();
    loop {
        if cancelled() {
            break;
        }
        crossbeam_channel::select! {
            recv(acc_rx) -> msg => match msg {
                Ok(slice) => step(AccWindowStep::Slice(slice)),
                // 发送端关闭 = 松键 ⇒ 退出，交给收尾 drain 收齐在飞窗口。
                Err(_) => break,
            },
            recv(res_rx) -> msg => match msg {
                Ok(result) => step(AccWindowStep::Result(result)),
                // 收尾段之外 res_rx 不该断开（worker 未全部退出前持有发送端）；
                // 真断开按「不再有结果」处理，避免死循环。
                Err(_) => break,
            },
            recv(partial_rx) -> msg => match msg {
                Ok(p) => step(AccWindowStep::Partial(p)),
                Err(_) => partial_rx = crossbeam_channel::never(),
            },
        }
    }
}

/// FIX-TAIL-WINDOW-AND-FALLBACK-386（C）：窗口文本选择 —— 解码文本为空（含解码 Err）⇒ 用**本窗流式文本**
/// 兜底；流式也为空才保持空。防结尾整段丢失（本次 seq7 `<location>` 丢 40+ 字）。纯函数，可单测。
/// FIX-SPLIT-SLICE-STREAMING-411：一次派发切成多片时，整段流式文本按**各片样本占比**分配到各片。
///
/// 旧实现（386 补）「`k == 0` 返回整段、其余返回空」会让**只含第一片**的窗口拿到整段文本 ⇒
/// 406 比对基准含窗口外内容 ⇒ 正确精解被误拒（BUILD-409 16:44Z：窗#0 retention 0.38 误拒），
/// 且整段被兜底回灌 ⇒ 最终文本**重复**。
///
/// 分配口径（与 `tail_streaming_baseline` 一致）：
/// - 边界 `boundary(cum) = round(total_chars × cum / total_samples)`，对**同一累计样本单一定义**
///   ⇒ 各片 `end == 下一片 start`、首 0、末 `total_chars` ⇒ 各片字数**求和恒等于总字数**（不丢不重）；
/// - **按 char 切**（`chars().skip/take`），禁字节下标（`LOCALRT-CHARBOUNDARY-344`）。
///
/// `slice_samples` = 本次派发各片的**样本数**（时间序），`k` 为片下标；总样本 0 / 空文本 / k 越界 ⇒ 空串。
fn slice_streaming_text(k: usize, seg_streaming: &str, slice_samples: &[usize]) -> String {
    let total: u64 = slice_samples.iter().map(|s| *s as u64).sum();
    let total_chars = seg_streaming.chars().count();
    if total == 0 || total_chars == 0 || k >= slice_samples.len() {
        return String::new();
    }
    let cum_before: u64 = slice_samples[..k].iter().map(|s| *s as u64).sum();
    let cum_after = cum_before + slice_samples[k] as u64;
    let boundary =
        |cum: u64| -> usize { ((total_chars as u64 * cum + total / 2) / total) as usize };
    let (start, end) = (boundary(cum_before), boundary(cum_after));
    seg_streaming
        .chars()
        .skip(start)
        .take(end - start)
        .collect()
}

fn window_text_with_fallback(decoded: &str, streaming: &str) -> String {
    if decoded.is_empty() {
        streaming.to_string()
    } else {
        decoded.to_string()
    }
}

/// FIX-VOICEPRINT-FALLBACK-421：本窗最终文本决策（纯函数，可单测）。
///
/// - `full_drop`（声纹整窗全剔）⇒ **输出空、`is_fallback=false`**（绝不走流式兜底把他人语音放回）。
/// - 否则：精解被 406 拒（`!accept` 且非空）⇒ 用**保留部分流式**兜底；精解为空（模型自身空，未剔除）
///   ⇒ 仍按 386-C 用保留部分流式兜底；精解被采纳 ⇒ 用精解。
/// 返回 `(is_fallback, 最终文本)`。
fn window_final_text(
    decoded: &str,
    kept_streaming: &str,
    accept: bool,
    full_drop: bool,
) -> (bool, String) {
    if full_drop {
        return (false, String::new());
    }
    let from_streaming = !decoded.is_empty() && !accept;
    let is_fallback = from_streaming || decoded.is_empty();
    let text = if from_streaming {
        kept_streaming.to_string()
    } else {
        window_text_with_fallback(decoded, kept_streaming)
    };
    (is_fallback, text)
}

/// FALLBACK-EXCLUDE-FARFIELD-446：兜底裁剪用的「保留 / 剔除」区间 —— 把**远场**区间并入剔除、从保留中减去。
///
/// Gavin 2026-09-26 端测：旁放他人语音，精解对这段只出「Imagem」被 406 拒，退回流式兜底，
/// 流式把背景声误识别成「在才能在他的的操作搞成一务，当don't…」写进最终文字。近场门已判这些语音为远场
///（`vad_only_speech_chunks=232`），但 392 起门不定内容去留 ⇒ 只在**兜底**这一步借用远场信息。
/// - `far` 空 ⇒ `None`（兜底与改前逐位相同）；
/// - `kept ∪ dropped` 空（声纹未运行、无语音轴）⇒ `None`（语音轴未知时绝不删字，宁留勿吞）；
/// - 否则返回 `(kept − far, dropped ∪ far)`，交 [`kept_streaming_text`] 删落在远场的字。纯函数，可单测。
fn fallback_ranges_with_far(
    kept: &[(usize, usize)],
    dropped: &[(usize, usize)],
    far: &[(usize, usize)],
) -> Option<(Vec<(usize, usize)>, Vec<(usize, usize)>)> {
    if far.is_empty() || (kept.is_empty() && dropped.is_empty()) {
        return None;
    }
    let mut kept2: Vec<(usize, usize)> = Vec::new();
    for &(a, b) in kept {
        let mut pieces = vec![(a, b)];
        for &(fa, fb) in far {
            let mut next = Vec::new();
            for (pa, pb) in pieces {
                if fb <= pa || fa >= pb {
                    next.push((pa, pb));
                } else {
                    if fa > pa {
                        next.push((pa, fa));
                    }
                    if fb < pb {
                        next.push((fb, pb));
                    }
                }
            }
            pieces = next;
        }
        kept2.extend(pieces);
    }
    let mut dropped2 = dropped.to_vec();
    dropped2.extend_from_slice(far);
    Some((kept2, dropped2))
}

/// FIX-VOICEPRINT-FALLBACK-421 / R1：把**本窗流式文本**按声纹剔除区间裁剪 —— **只删被剔除的字**。
///
/// - `dropped_ranges` 空 ⇒ 无剔除（或仅保留）⇒ `streaming` **原样返回**（逐位不变）。
/// - **时间戳路径**：`timestamps` 给出每个字的**窗口内秒数**且长度 == 字数 ⇒ 落在 `dropped_ranges`
///   的字删除，其余（含无法归属）保留。
/// - **估算路径**：无时间戳 ⇒ 把字**只铺在语音时间轴**（`kept ∪ dropped` 裁剪到本段 `[offset, offset+span)`）上，
///   第 `i` 字落到该轴 `(i+0.5)/N` 处 → 换回窗内样本；**落在 dropped 才删，其余一律保留**。
///
/// 🔴 R1 要点：旧版把字**均匀铺满整个窗口时长（含静音）**，本人的字会被映射进静音缝 ⇒ 误删（吞本人字）。
/// 铺在**语音轴**上纠正之；判定改成「**只删 dropped**」⇒ 偏保留、绝不吞字（Gavin 底线）。
///
/// `span_samples` = 这段流式对应音频的样本数；`offset_samples` = 其在**窗内**的起始样本
///（常规窗 0；410 末尾窗 pending 段 = 窗总样本 − pending 样本）。纯函数，可单测。
fn kept_streaming_text(
    streaming: &str,
    span_samples: usize,
    offset_samples: usize,
    kept_ranges: &[(usize, usize)],
    dropped_ranges: &[(usize, usize)],
    timestamps: Option<&[f32]>,
) -> String {
    if dropped_ranges.is_empty() || streaming.is_empty() {
        return streaming.to_string();
    }
    let chars: Vec<char> = streaming.chars().collect();
    let n = chars.len();
    if n == 0 {
        return String::new();
    }
    let in_dropped = |sample: usize| {
        dropped_ranges
            .iter()
            .any(|&(a, b)| sample >= a && sample < b)
    };
    // 时间戳路径（逐字窗口内秒数）：只删落在 dropped 的字。
    if let Some(ts) = timestamps {
        if ts.len() == n {
            let mut out = String::with_capacity(streaming.len());
            for (i, &c) in chars.iter().enumerate() {
                if !in_dropped((ts[i].max(0.0) * 16000.0) as usize) {
                    out.push(c);
                }
            }
            return out;
        }
    }
    // 估算路径：字只铺在**语音轴**上（kept ∪ dropped，裁剪到本段），落在 dropped 才删。
    let (lo, hi) = (offset_samples, offset_samples.saturating_add(span_samples));
    let mut speech: Vec<(usize, usize)> = kept_ranges
        .iter()
        .chain(dropped_ranges.iter())
        .filter_map(|&(a, b)| {
            let (a2, b2) = (a.max(lo), b.min(hi));
            if b2 > a2 {
                Some((a2, b2))
            } else {
                None
            }
        })
        .collect();
    speech.sort_unstable();
    let total_speech: usize = speech.iter().map(|(a, b)| b - a).sum();
    if span_samples == 0 || total_speech == 0 {
        return streaming.to_string(); // 无语音轴信息 ⇒ 偏保留
    }
    let mut out = String::with_capacity(streaming.len());
    for (i, &c) in chars.iter().enumerate() {
        let pos = ((i as f64 + 0.5) / n as f64 * total_speech as f64) as usize;
        let mut acc = 0usize;
        let mut sample = hi; // 越界 ⇒ 不属任何区间 ⇒ 保留
        for &(a, b) in &speech {
            let len = b - a;
            if pos < acc + len {
                sample = a + (pos - acc);
                break;
            }
            acc += len;
        }
        if !in_dropped(sample) {
            out.push(c);
        }
    }
    out
}

/// FIX-NEARFIELD-BY-SEGMENT-AND-PREVIEW-389（D3）：**部分窗**（不含本次派发末片）的 `committed_len` 折算。
///
/// `frac = 前 0..=k 片样本数 / 本次派发全部片样本数`；`win = prev_committed + round((committed_len −
/// prev_committed) × frac)`。部分窗若用整次派发的边界（覆盖被延后末片）会把预览截短。
fn partial_win_committed(
    prev_committed: usize,
    committed_len: usize,
    cum_samples: usize,
    total_samples: usize,
) -> usize {
    let frac = if total_samples > 0 {
        cum_samples as f32 / total_samples as f32
    } else {
        1.0
    };
    prev_committed
        + (((committed_len.saturating_sub(prev_committed)) as f32) * frac).round() as usize
}

/// 386：组窗计划（纯函数输出）。
///
/// LOCALRT-STOP-TAIL-UNIFY-423：收尾（松键）不再走本函数 —— 统一走 `emit_tail_window!`
/// （末尾组窗），故原「规则 4 短尾合并」与 `TAIL_MERGE_MAX_SECS` 一并删除。
#[derive(Debug, PartialEq, Eq)]
struct WindowPlan {
    /// 本次要**立刻派发**的窗口（全局切片区间 `[start,end)`，按序）。
    windows: Vec<(usize, usize)>,
    /// 更新后的「待覆盖片」全局下标（`None` = 无待覆盖）。
    pending: Option<usize>,
}

/// LOCALRT-TAIL-WINDOW-407：长静默末尾窗的**纯决策**。
///
/// `pending` = 待覆盖片全局下标；`prev_base` = `recent_slices[0]` 的全局下标；`recent_len` = 片数。
/// 有 pending 且换算到 `recent` 后**存在前片**（下标 `i > 0`）⇒ 返回
/// `(窗口 span 起点, 终点, 前片下标 i)` = `(p-1, p+1, i)`；否则 `None`（无 pending / 该片是首片）。
fn tail_window_span(
    pending: Option<usize>,
    prev_base: usize,
    recent_len: usize,
) -> Option<(usize, usize, usize)> {
    let p = pending?;
    let i = p.checked_sub(prev_base)?;
    if i > 0 && i < recent_len {
        Some((p - 1, p + 1, i))
    } else {
        None
    }
}

/// LOCALRT-STOP-TAIL-UNIFY-423：末尾窗「**单独解**」纯决策 —— `pending` 落在 `recent` 范围内但
/// **是首片**（无前片）⇒ 返回单独窗 span `(p, p+1)`；越界 / 无 pending ⇒ `None`。
///
/// 与 [`tail_window_span`] 配合：`span` 有解 ⇒ 前片后缀 + pending；`alone` 有解 ⇒ 单独解 pending。
/// 长静默与松键收尾**共用**（`emit_tail_window!`），不再按 pending 时长（<3s / ≥3s）分叉。
fn tail_window_alone(
    pending: Option<usize>,
    prev_base: usize,
    recent_len: usize,
) -> Option<(usize, usize)> {
    let p = pending?;
    let i = p.checked_sub(prev_base)?;
    if i < recent_len {
        Some((p, p + 1))
    } else {
        None
    }
}

/// LOCALRT-TAIL-WINDOW-407（R1）：末尾窗的**回溯时长**。
///
/// R1 起因（主控）：固定回溯 2s 在**慢速语音**下只覆盖 6~9 字，**低于对齐质量门
/// `ALIGN_MIN_OVERLAP_CHARS`（8）** ⇒ 对齐直接 fail ⇒ 末尾窗接缝**重复字**（历史反复出现的接缝重复）。
/// 改为按**本次录音已有字速**（[`transcription::acc_avg_chars_per_sec`] 的运行均值；冷启动无均值按
/// **3.0 字/秒**）估算出「至少 `ALIGN_MIN_OVERLAP_CHARS + 4` 个字」所需时长：
/// `max(2.0, (ALIGN_MIN_OVERLAP_CHARS + 4) / rate)`。Gavin 口径是「不短于 2s」，更长符合要求。
/// 调用方再把时长换算为样本并**封顶为整个前片**（前片不够取整片）。
fn tail_backtrack_secs(rate_cps: f32) -> f32 {
    let need = (transcription::ALIGN_MIN_OVERLAP_CHARS + 4) as f32; // 8 + 4 = 12 字
    let rate = if rate_cps.is_finite() && rate_cps > 0.0 {
        rate_cps
    } else {
        3.0
    };
    (need / rate).max(2.0)
}

/// FIX-TAIL-GUARD-410 item1：末尾窗的 **406 比对基准** = 前片流式的**末尾相应字符数**（按
/// `suffix_samples / prev_samples` 比例、**按 char 切**，禁字节下标，见 `LOCALRT-CHARBOUNDARY-344`）
/// + pending 流式。**不得**用整个前片流式（否则基准含窗口外内容 ⇒ 正确精解被 406 误拒）。
fn tail_streaming_baseline(
    prev_streaming: &str,
    suffix_samples: usize,
    prev_samples: usize,
    pending_streaming: &str,
) -> String {
    let prev_chars = prev_streaming.chars().count();
    let ratio = if prev_samples == 0 {
        0.0
    } else {
        suffix_samples as f32 / prev_samples as f32
    };
    let n = (((prev_chars as f32) * ratio).round() as usize).min(prev_chars);
    let mut s: String = prev_streaming.chars().skip(prev_chars - n).collect();
    s.push_str(pending_streaming);
    s
}

/// LOCALRT-SHORT-CONTEXT-413：本窗「必须完整解的片」起点 `must_start`（纯函数，可单测）。
///
/// - 本批**首个**窗口且上一批遗留待覆盖片 `p` 落在窗内 `[gs,ge)` ⇒ `p`
///   （`plan_windows` 规则 3 把 pending 并进来：pending 到本窗末片都完整解）；
/// - 其余（常态单片 / 多片切分的后续子窗 / 无 pending）⇒ 窗末片 `ge-1`。
///
/// 背景：常规窗原先按 `group_window_start_secs` 把前面整片一起带上 ⇒ 末句虽只 2.85s，
/// 送解却 9.33s（BUILD-399 09:48:21Z「开心开」，`decode_ms=2194`）。派发时**无法知道
/// 这是不是最后一句**，故只能所有常规窗都少带前文（Gavin 2026-09-25）。
fn must_start_for_window(
    gs: usize,
    ge: usize,
    pending_before: Option<usize>,
    is_first: bool,
) -> usize {
    if is_first {
        if let Some(p) = pending_before {
            if p >= gs && p < ge {
                return p;
            }
        }
    }
    ge.saturating_sub(1)
}

/// LOCALRT-ALWAYS-CONTEXT-427：本窗「前文后缀」取自哪一片（纯函数，可单测）。
///
/// Gavin 2026-09-25：「不管新句多长，只要本次录音前面还有片，就从前一片末尾切一截带上；
/// 10s 上限只决定前面整片要不要一起重解，不再影响带不带末尾一截」。
///
/// 413 原判据 `must_start > gs` 在**新片长**时（`group_window_start_secs` 把前片排除 ⇒ `gs == must_start`）
/// 返回 `None` ⇒ 长句窗无前文（日志 `[LocalRT-DBG-413] prev_cut_secs=0.00`）。427 改为：只要紧邻前一片
/// （`must_start-1`）**仍在 `recent_slices` 缓冲内**（`>= buf_base`）就取其后缀，**不再要求它落在
/// `[gs, ge)` 内**。首片 / 前片已被缓冲裁掉 ⇒ `None`。返回**全局切片下标**，调用方换算到局部下标。
///
/// `gs < must_start-1`（窗内还有更早的整片）⇒ 与 413 相同：更早整片丢弃，只留紧邻前一片后缀。
fn short_context_span(ge: usize, must_start: usize, buf_base: usize) -> Option<usize> {
    if must_start >= 1 && must_start < ge && must_start - 1 >= buf_base {
        Some(must_start - 1)
    } else {
        None
    }
}

/// LOCALRT-SHORT-CONTEXT-413：把某片的片内语音区间裁剪到 `[cut, len)` 并平移 `-cut`，
/// 得到该片**后缀**在窗内的坐标（纯函数，可单测）。
///
/// 跨 `cut` 的区间被截断（起点抬到 `cut`）；完全落在 `cut` 之前（`b <= cut`）的被丢。
/// 与 [`shift_and_concat_ranges`] 配套：先裁剪平移前文后缀区间，再由后者按各片样本数偏移拼接。
fn trim_and_shift_ranges(ranges: &[(usize, usize)], cut: usize) -> Vec<(usize, usize)> {
    ranges
        .iter()
        .filter_map(|&(a, b)| {
            if b <= cut {
                None
            } else {
                Some((a.max(cut) - cut, b - cut))
            }
        })
        .collect()
}

/// LOCALRT-SHORT-CONTEXT-413 / LOCALRT-TAIL-WINDOW-407 **共用**的前文后缀提取结果。
struct ContextSuffix {
    /// 前片内切点（`prev[cut..]` 为送入的前文后缀）。
    cut: usize,
    /// 后缀样本数（`prev.len() - cut`）。
    suffix_samples: usize,
    /// 后缀对应的流式**字符数**（仅日志）。
    suffix_chars: usize,
    /// 回溯时长（秒，仅日志）。
    backtrack_secs: f32,
    /// 是否在回溯区间内找到真「字缝」（仅日志）。
    gap_found: bool,
    /// 406 比对基准 = 前片流式末尾相应字数 + must 流式。
    baseline: String,
}

/// FORCED-ALIGN-456：窗口在录音虚拟时间轴上的 `(绝对起点秒, 与上一窗分界绝对秒)`（纯函数）。
///
/// 窗口音频在第 `ge-1` 片末尾结束 ⇒ 起点 = `slice_cum_end[ge-1] − Σsamples`。与上一窗共享
/// `prev_we − span_start` 段（重叠区）；分界 = 重叠区起点 + `split_frac × 重叠样本`（436 同源），
/// 无分界比例 ⇒ 重叠区终点（新片起点）；无重叠 ⇒ 窗口起点。
fn window_abs_geometry(
    slice_cum_end: &[usize],
    ge: usize,
    span_start: usize,
    prev_span_end: Option<usize>,
    samples: &[usize],
    split_frac: Option<f32>,
) -> (f32, f32) {
    let end = ge
        .checked_sub(1)
        .and_then(|i| slice_cum_end.get(i))
        .copied()
        .unwrap_or(0);
    let start = end.saturating_sub(samples.iter().sum());
    let shared = prev_span_end
        .map_or(0, |pe| pe.saturating_sub(span_start))
        .min(samples.len());
    let shared_samples: usize = samples[..shared].iter().sum();
    let split = start as f32
        + split_frac.map_or(shared_samples as f32, |f| {
            f.clamp(0.0, 1.0) * shared_samples as f32
        });
    (start as f32 / 16000.0, split / 16000.0)
}

/// SEAM-INTERIOR-ONLY-436：本窗与上一窗重叠区的内部分界比例（相对重叠区起点，∈(0,1)）。
///
/// 重叠区音频 = 本窗音频开头的共享切片部分（`samples[..shared]`，`shared = prev_we − span_start`）。
/// 分界点算法见 [`transcription::local_stream::interior_split_frac`]（中间 40%~60% 最近真停顿，
/// 无 ⇒ 中点）。无共享（首窗 / `span_start >= prev_we` / 空音频）⇒ None。
/// 纯函数（`prev_span_end` 由调用方取上一窗 `we`）。mod.rs 接线待 436 第二阶段。
fn window_overlap_split(
    span_start: usize,
    prev_span_end: Option<usize>,
    audio: &[f32],
    samples: &[usize],
) -> Option<f32> {
    let pe = prev_span_end?;
    let shared = pe.saturating_sub(span_start).min(samples.len());
    if shared == 0 {
        return None;
    }
    let overlap: usize = samples[..shared].iter().sum();
    let overlap = overlap.min(audio.len());
    if overlap == 0 {
        return None;
    }
    Some(transcription::local_stream::interior_split_frac(
        &audio[..overlap],
    ))
}

/// LOCALRT-SHORT-CONTEXT-413：从**紧邻前一片**取「后缀」作为本窗前文（407 / 413 共用，
/// **禁止复制出第二份**）。
///
/// 回溯时长按 [`tail_backtrack_secs`]（本次字速自适应，冷启动 3.0 字/秒）→ 换算样本并
/// **封顶整片**；再用 [`transcription::local_stream::find_tail_cut`]（与 407 同一套字缝判据）
/// 找切点，无字缝回落回溯点。`suffix_samples` 与 `baseline` 供常规窗 / 407 末尾窗记账。
fn take_context_suffix(
    prev: &[f32],
    prev_stream: &str,
    must_stream: &str,
    rate_cps: f32,
) -> ContextSuffix {
    let backtrack_secs = tail_backtrack_secs(rate_cps);
    let back = ((backtrack_secs * 16000.0) as usize).min(prev.len()).max(1);
    let (cut, gap_found) = transcription::local_stream::find_tail_cut(prev, back);
    let suffix_samples = prev.len() - cut;
    let prev_chars = prev_stream.chars().count();
    let ratio = if prev.is_empty() {
        0.0
    } else {
        suffix_samples as f32 / prev.len() as f32
    };
    let suffix_chars = (((prev_chars as f32) * ratio).round() as usize).min(prev_chars);
    let baseline = tail_streaming_baseline(prev_stream, suffix_samples, prev.len(), must_stream);
    ContextSuffix {
        cut,
        suffix_samples,
        suffix_chars,
        backtrack_secs,
        gap_found,
        baseline,
    }
}

/// LOCALRT-SHORT-CONTEXT-413（R1）：**带前文**常规窗的 410 兜底元数据 —— 只兜底 **must 片**。
///
/// 精解被 406 拒 / 为空时用的兜底文本/区间只覆盖 `(must_start, ge)`；前文后缀
/// `must_start-1` 是上一窗**已精解过的准确文本**，不得被流式兜底顶掉（同 410 item2）。
fn context_tail_pending(
    must_start: usize,
    ge: usize,
    must_samples: Vec<usize>,
    must_streaming: String,
) -> TailPending {
    TailPending {
        span: (must_start, ge),
        samples: must_samples,
        streaming: must_streaming,
    }
}

/// FIX-TAIL-WINDOW-AND-FALLBACK-386：组窗纯函数（**中途派发**用）。
///
/// 输入：`prev_durs` 已派发片时长（时间序，调用方保证已按 `WINDOW_MAX_SLICES` 裁剪）；
/// `prev_base` = `prev_durs[0]` 的全局下标；`new_durs` = 本次派发携带的新片时长（时间序）；
/// `pending` = 上一次遗留的**待覆盖片**全局下标（`None` = 无）。
///
/// 规则（Gavin 2026-09-23 BUILD-385 端测）：
/// 1. 单次派发 **1 片**（常态）⇒ 与旧行为相同，立刻组窗（含把 `pending` 强制纳入）。
/// 2. 单次派发 **≥2 片**（被切分）⇒ 除**最后一片**外每片立刻组窗；最后一片不组窗，记为待覆盖片。
/// 3. 有 `pending` 时，**本批第一个窗口起点强制 ≤ pending**（把待覆盖片纳入，**哪怕总长超 `WINDOW_MAX_SECS`**）。
///
/// 🔴 LOCALRT-STOP-TAIL-UNIFY-423：**收尾（松键）不再走本函数** —— 松键有 pending 时统一走
/// `emit_tail_window!`（末尾组窗：前片后缀 + pending），与 407 长静默机制一致；原「规则 4 短尾合并」
/// 与 `is_tail` 参数、`TAIL_MERGE_MAX_SECS` 一并删除。本函数只服务**中途切片派发**。
fn plan_windows(
    prev_durs: &[f32],
    new_durs: &[f32],
    prev_base: usize,
    pending: Option<usize>,
) -> WindowPlan {
    let mut buf: Vec<f32> = prev_durs.to_vec();
    let mut base = prev_base; // buf[0] 的全局下标
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut pend = pending;
    let n = new_durs.len();

    for (k, nd) in new_durs.iter().enumerate() {
        buf.push(*nd);
        while buf.len() > transcription::WINDOW_MAX_SLICES {
            buf.remove(0);
            base += 1;
        }
        let end = base + buf.len();
        let is_last = k + 1 == n;
        // 规则 2：一次派发 ≥2 片 ⇒ 最后一片**延后**（收尾由 `emit_tail_window!` 覆盖）。
        if is_last && n >= 2 {
            pend = Some(end - 1);
            continue;
        }
        let mut start =
            base + transcription::group_window_start_secs(&buf, transcription::WINDOW_MAX_SECS);
        // 规则 3：本批第一个窗口强制纳入待覆盖片（允许超 WINDOW_MAX_SECS）。
        if let Some(p) = pend {
            if p < start {
                start = p;
            }
            pend = None;
        }
        out.push((start, end));
    }

    WindowPlan {
        windows: out,
        pending: pend,
    }
}

/// 382（3A）：一次状态机推进的结果。
// ASR-SERVER-491：去 Windows 限定 —— 远程会话（两端内核都有）复用本状态机。
#[derive(Debug, PartialEq, Eq)]
enum ReflowFastOutcome {
    /// 无动作（例如边界到、但对应 seg 已不是最新已渲染）。
    None,
    /// 应更新权威状态；`render=true` 才重画浮层（`false` = Processing 已开始，只更新状态不重画）。
    Apply {
        text: String,
        committed_len: usize,
        accurate: bool,
        render: bool,
    },
}

/// 382（3A）：`replace_all=true` 回灌的快速路径状态机。
///
/// 背景：325/337 时代 `PreviewReflow` 只登记全文、必须等同片 `ReflowCommit` 边界配对才渲染；
/// 现在唯一发送点恒 `replace_all=true`（整段替换），「等边界」只会**白白延迟**（日志：1 次多等 0.4s、
/// 2 次单槽被覆盖永不配对、1 次 `boundary=b` 整次扣下）。本状态机：全文一到**立即**用「已知边界 or
/// 派发当刻 `committed_len`」渲染；边界后到且该 seg 仍是最新已渲染 ⇒ 用准确边界再渲染一次。
// ASR-SERVER-491：去 Windows 限定（同上：`ReflowFastOutcome`）。
#[derive(Default)]
struct ReflowFastState {
    /// 最新登记全文 + 派发当刻 fallback：`(gen, seg, text, fallback_len, boundary_usable)`。
    /// `boundary_usable=false`（部分窗）⇒ 不用 `bound_of` 的整次派发边界、且晚到边界不重渲。
    latest: Option<(u64, usize, String, usize, bool)>,
    /// 边界登记 `(gen, seg) -> Option<usize>`（`None` = b 类「本片无准确边界」）。小 map，按代清空。
    bounds: Vec<((u64, usize), Option<usize>)>,
    /// 最近一次**应渲染**的 `(gen, seg)`（判断晚到边界是否该重渲染）。
    rendered: Option<(u64, usize)>,
}

// ASR-SERVER-491：去 Windows 限定（同上）。
impl ReflowFastState {
    /// ACC-452 ⑤：流式回灌渲染了 `(gen, seg)` 的半截结果 ⇒ 登记为最近渲染，
    /// 使**上一窗**晚到的边界不再用旧全文重画（否则盖掉半截结果 = 回缩）。
    fn note_partial(&mut self, gen: u64, seg: usize) {
        self.rendered = Some((gen, seg));
    }

    fn clear(&mut self) {
        self.latest = None;
        self.bounds.clear();
        self.rendered = None;
    }

    fn bound_of(&self, gen: u64, seg: usize) -> Option<Option<usize>> {
        self.bounds
            .iter()
            .rev()
            .find(|((g, s), _)| *g == gen && *s == seg)
            .map(|(_, v)| *v)
    }

    /// 收到一条权威全文（已过 `reflow_action` 的编辑/取消/陈旧/洞/空判据）。
    /// `boundary_usable`：见 `PreviewReflow`——`false`（部分窗）时**不用** `bound_of` 的整次派发边界，
    /// 直接用 `fallback_len`（按片样本占比折算的 `win_committed`）。
    /// `suppressed` = 本代已进入 Processing（只跳过重画，权威状态照常更新）。
    fn on_text(
        &mut self,
        gen: u64,
        seg: usize,
        text: String,
        fallback_len: usize,
        boundary_usable: bool,
        suppressed: bool,
    ) -> ReflowFastOutcome {
        self.latest = Some((gen, seg, text.clone(), fallback_len, boundary_usable));
        self.rendered = Some((gen, seg));
        let (len, accurate) = if boundary_usable {
            match self.bound_of(gen, seg) {
                Some(Some(l)) => (l, true),
                _ => (fallback_len, false),
            }
        } else {
            // 部分窗：`bound_of` 若为整次派发边界（覆盖被延后末片）会截短预览 ⇒ 用折算值。
            (fallback_len, false)
        };
        ReflowFastOutcome::Apply {
            text,
            committed_len: len,
            accurate,
            render: !suppressed,
        }
    }

    /// 收到一条边界。若对应 seg 是**最新已渲染**全文 **且该次渲染边界可用**（非部分窗）⇒ 用准确边界再渲染一次。
    fn on_bound(
        &mut self,
        gen: u64,
        seg: usize,
        len: Option<usize>,
        suppressed: bool,
    ) -> ReflowFastOutcome {
        if let Some(e) = self
            .bounds
            .iter_mut()
            .find(|((g, s), _)| *g == gen && *s == seg)
        {
            e.1 = len;
        } else {
            self.bounds.push(((gen, seg), len));
        }
        if self.rendered == Some((gen, seg)) {
            if let (Some(l), Some((g2, s2, text, _, usable))) = (len, self.latest.as_ref()) {
                // 🔴 D3：部分窗的最新渲染**不得**用整次派发边界重渲（否则截短预览）。
                if (*g2, *s2) == (gen, seg) && *usable {
                    return ReflowFastOutcome::Apply {
                        text: text.clone(),
                        committed_len: l,
                        accurate: true,
                        render: !suppressed,
                    };
                }
            }
        }
        ReflowFastOutcome::None
    }
}

/// PUNCT-478（DEC-108，Gavin 10-04 端测「……差异，但是。根本的法则……」）：在线识别服务按停顿断句，
/// 在句间接缝自动收句末标点，而那里话没说完。去掉服务端在接缝处加的句读标点，用本地标点模型
/// （中英 CT / 日韩 jako）按接缝前后文重判（逗号 / 句号 / 不加）；其余位置标点与最后一句句末不动。
/// 没有标点引擎（用户关了自动标点）⇒ 原样返回（后面 L2 节点本就会去标点）。
fn repair_online_seams(
    text: &str,
    seams: &[usize],
    engine: Option<&mut punctuation::PunctuationEngine>,
) -> String {
    let Some(engine) = engine else {
        return text.to_string();
    };
    repair_seams_with(text, seams, |ctx| {
        if punctuation::ct_unsupported_script(ctx) {
            punctuation::jako::punctuate(ctx)
        } else {
            engine.add_punctuation(ctx)
        }
    })
}

/// PUNCT-478：接缝重判主体（`punctuate` = 给一段上下文整段打标点；测试可注入假模型）。
fn repair_seams_with(
    text: &str,
    seams: &[usize],
    mut punctuate: impl FnMut(&str) -> Option<String>,
) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut last = 0usize;
    for &seam in seams {
        if seam <= last || seam >= text.len() || !text.is_char_boundary(seam) {
            continue;
        }
        let left = &text[last..seam];
        let core = left.trim_end_matches(punctuation::is_seam_mark);
        let original = &left[core.len()..];
        out.push_str(core);
        let right = &text[seam..];
        let judged = punctuation::judge_seam(&out, right, &mut punctuate);
        match judged {
            Some(marks) => {
                // 英文两词之间去掉句号后补一个空格，免得粘成一个词。
                if marks.is_empty()
                    && core
                        .chars()
                        .last()
                        .is_some_and(|c| c.is_ascii_alphanumeric())
                    && right
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_alphanumeric())
                {
                    out.push(' ');
                }
                if marks != original {
                    log::info!("PUNCT-478: online seam {original:?} -> {marks:?}");
                }
                out.push_str(&marks);
            }
            None => out.push_str(original),
        }
        last = seam;
    }
    out.push_str(&text[last..]);
    out
}

/// PUNCT-INTEGRATION-001 + ASR-PUNCT-OPT-001: 本地标点决策（加标点）。
/// 条件：auto_punct=true && LLM 未处理 && 非翻译 && 非 native 自带标点。
/// - native_punctuated=true（accuracy native 成功）→ 跳过标点引擎（省一次推理）
/// - native_punctuated=false（performance/兜底/混合）→ 照常走标点引擎
/// - 本函数只负责「加标点」；「剥离」职责在下游 L2 后处理补位块
///   （PUNCT-GOVERNANCE-030-A），对全部产出源一视同仁，无来源判据。
/// PIPELINE-ORCH-238: 从 run_pipeline_core 内联块原样提取，行为逐位不变。
fn apply_local_punctuation(
    final_text: String,
    enabled: bool,
    llm_handled: bool,
    translate_requested: bool,
    native_punctuated: bool,
    engine: Option<&mut punctuation::PunctuationEngine>,
) -> String {
    if enabled && !llm_handled && !translate_requested && !native_punctuated {
        // KOJA-PUNCT-464：含日文假名 / 韩文 ⇒ 不送 CT（中英词表，韩文空格会被吃光、日文只补句末「。」）。
        // PUNCT-JAKO-466（Gavin 10-03，含 performance 档）：改用日韩专用标点（首次遇到日韩文才加载）；
        // 模型不可用 / 出错 ⇒ 原样保留。中英文逐位不变（仍走下方 CT）。
        if punctuation::ct_unsupported_script(&final_text) {
            log::info!("Local punctuation skipped: ja/ko text not supported by punctuation model");
            return match punctuation::jako::punctuate(&final_text) {
                Some(punctuated) => {
                    log::info!(
                        "Local ja/ko punctuation applied: '{}' -> '{}'",
                        final_text,
                        punctuated
                    );
                    punctuated
                }
                None => final_text,
            };
        }
        if let Some(engine) = engine {
            match engine.add_punctuation(&final_text) {
                Some(punctuated) => {
                    log::info!(
                        "Local punctuation applied: '{}' -> '{}'",
                        final_text,
                        punctuated
                    );
                    punctuated
                }
                None => {
                    log::warn!("Local punctuation returned None, keeping original text");
                    final_text
                }
            }
        } else {
            log::debug!("Punctuation engine not available, skipping");
            final_text
        }
    } else {
        final_text
    }
}

/// PUNCT-FINAL-REDO-350：标点剥离**节点**（独立、管线无关、谁要谁挂）。
///
/// 形态照 [`apply_filler_strip`]（自由函数、显式传参、管线无关）；判定体是
/// `punctuation::strip_effective_punctuation`（可单测的纯函数）。
/// 🔴 **只挂本地 realtime 一条管线**（Gavin 2026-09-22 细化设计），在线 realtime 与
/// 本地离线**不挂**，序列一字不变 ⇒ 结构上不可能受影响（DEC-066：节点各自编排、谁要谁挂）。
///
/// 语义：把整段已有标点剥光，交给下游**既有的**标点节点（`apply_local_punctuation`）
/// 全量重打。剥光后 `native_punctuated` 恒 false ⇒ 下游 `!native_punctuated` 门自己放行；
/// 而该门本身**一行都不改**，`PUNCT-DOUBLE-334` 对其它两档的保护原样保留。
/// 先剥光 ⇒ 进引擎的文本无标点 ⇒ 结构上不可能再出现 `。。`/`，。` 叠加。
///
/// - `enabled == false` ⇒ **原样返回**（用户关闭标点：本节点无意义，下游 L2 会全文剥）
/// - `enabled == true`  ⇒ `strip_effective_punctuation(&final_text)`
///
/// ⚠️ 仅在「下游必定重打标点」时才可挂：见调用点关于 `!translate_requested` 的门注释。
/// 变化时打一行 `log::info!`（与 [`apply_local_punctuation`] / [`apply_filler_strip`] 同风格）。
fn strip_punctuation_node(final_text: String, enabled: bool) -> String {
    if !enabled {
        return final_text;
    }
    let stripped = punctuation::strip_effective_punctuation(&final_text);
    if stripped != final_text {
        log::info!("Punctuation strip node: '{}' -> '{}'", final_text, stripped);
    }
    stripped
}

fn apply_authoritative_filler_dedup(text: &str) -> String {
    let t0 = std::time::Instant::now();
    let stripped = text_normalizer::strip_fillers_conservative(text);
    if log::log_enabled!(log::Level::Debug) {
        log::debug!(
            "[DBG-441] filler once: chars={} ms={:.3}",
            text.chars().count(),
            t0.elapsed().as_secs_f64() * 1000.0
        );
    }
    if stripped != text {
        log::info!("Authoritative filler dedup: '{}' -> '{}'", text, stripped);
    }
    stripped
}

// MACOS-P4-NEUTRAL-001: 原 #[cfg(target_os = "windows")] 去除——平台中立纯 Rust，run_pipeline_core 调用，对 Windows 为 no-op。
fn should_try_llm_translate(llm_enabled: bool, connectivity_verified: bool) -> bool {
    llm_enabled && connectivity_verified
}

/// LANG-AUTO-001: 翻译方向按内容（contains_han）判定，不依赖 transcription_language 配置。
/// - target=English → 含汉字才需翻译；纯英文文本无需翻译。
/// - target=Chinese → 不含汉字才需翻译；纯英文文本需翻译。
// MACOS-P4-NEUTRAL-001: 原 #[cfg(target_os = "windows")] 去除——平台中立纯 Rust，run_pipeline_core 调用，对 Windows 为 no-op。
fn try_nllb_translate(
    text: &str,
    engine: Option<&translation::TranslationEngine>,
) -> Option<String> {
    let Some(engine) = engine else {
        log::info!("NLLB translation skipped: offline engine unavailable");
        return None;
    };
    match engine.translate(text) {
        Ok(translated) if !translated.trim().is_empty() => {
            log::info!("NLLB translation done: {}", translated);
            Some(translated)
        }
        Ok(_) => {
            log::warn!("NLLB translation returned empty text");
            None
        }
        Err(err) => {
            log::warn!("NLLB translation failed: {}", err);
            None
        }
    }
}

// =====================================================================
// FIX-GUARD-301：结构护栏共享「生产区」扫描文本提取
// ---------------------------------------------------------------------
// 三次复发教训：用「第一个出现的某标记」当区域边界，任何人在前面插一个同类
// 标记就塌缩（guard291 的 `} else`、295 预见的中途 cfg(test)、本次 298 插入
// 的 `mod parallel_acc_298_tests`）。故本 helper **不再截断**，而是**剔除全部
// test-gated 项、保留其余全文**，从语义上消除「位置敏感」。
//
// 🔴 test 判据不止 `#[cfg(test)]`：本文件另有 8 处
// `#[cfg(all(test, target_os = "windows"))]` 测试模块（9878/11529/11728/11953/
// 12434/12682/13408/13614），旧实现在 9165 就截断、从未扫到它们；改剔除法后若
// 只认 `#[cfg(test)]`，这 8 个测试模块会被当成生产区扫进去 ⇒ 假红。故判据取
// 「以 `#[cfg(` 开头 且含 `test` 且不含 `not(test)`」：
//   - `#[cfg(test)]`                          → 剔除
//   - `#[cfg(all(test, target_os = "windows"))]` → 剔除
//   - `#[cfg(target_os = "windows")]`（生产）  → 保留（不含 test）
//   - `#[cfg(not(test))]`（生产侧）            → 保留
// =====================================================================
#[cfg(test)]
mod guard_prod_lines {
    /// 单个 cfg 属性行是否把紧随的项 gate 成 test-only。
    fn is_test_gated_attr(t: &str) -> bool {
        let t = t.trim();
        t.starts_with("#[cfg(") && t.contains("test") && !t.contains("not(test)")
    }

    /// 🔴 FIX-GUARD-PRODLINES-422：**字符串 / 字符 / 注释感知**的顶层词法扫描。
    ///
    /// 从 `from` 起返回第一个「顶层」遇到的 `{` / `}` / `;` 的 `(字节下标, 字符)`；
    /// 跳过：普通字符串 `"…"`（含 `\"`）、原始字符串 `r"…"` / `r#"…"#`（任意 `#` 数）、
    /// 字符字面量 `'x'` / `'\''` / `'\u{…}'`（与生命周期 `'a` 区分）、`//` 行注释、
    /// `/* */` 块注释（**可嵌套**）。到末尾无顶层词 ⇒ `(len, None)`。
    ///
    /// 这是 `prod_lines_excluding_cfg_test` 与 TEST-SYNC-420 `fn_body` **共用的唯一状态机**
    ///（禁两份；根治 `[GUARD-SKIP-BRACE-IN-STRING-382]`）。
    pub(crate) fn next_top_token(src: &str, from: usize) -> (usize, Option<char>) {
        let b = src.as_bytes();
        let n = b.len();
        let mut i = from;
        while i < n {
            match b[i] {
                b'/' if i + 1 < n && b[i + 1] == b'/' => {
                    i += 2;
                    while i < n && b[i] != b'\n' {
                        i += 1;
                    }
                }
                b'/' if i + 1 < n && b[i + 1] == b'*' => {
                    i += 2;
                    let mut d = 1i32;
                    while i < n && d > 0 {
                        if b[i] == b'/' && i + 1 < n && b[i + 1] == b'*' {
                            d += 1;
                            i += 2;
                        } else if b[i] == b'*' && i + 1 < n && b[i + 1] == b'/' {
                            d -= 1;
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                }
                b'"' => {
                    i += 1;
                    while i < n {
                        if b[i] == b'\\' {
                            i += 2;
                        } else if b[i] == b'"' {
                            i += 1;
                            break;
                        } else {
                            i += 1;
                        }
                    }
                }
                b'r' if i + 1 < n && (b[i + 1] == b'"' || b[i + 1] == b'#') => {
                    let mut j = i + 1;
                    let mut hashes = 0usize;
                    while j < n && b[j] == b'#' {
                        hashes += 1;
                        j += 1;
                    }
                    if j < n && b[j] == b'"' {
                        j += 1;
                        loop {
                            if j >= n {
                                break;
                            }
                            if b[j] == b'"' {
                                let mut k = j + 1;
                                let mut cnt = 0usize;
                                while k < n && b[k] == b'#' && cnt < hashes {
                                    cnt += 1;
                                    k += 1;
                                }
                                if cnt == hashes {
                                    j = k;
                                    break;
                                }
                            }
                            j += 1;
                        }
                        i = j;
                    } else {
                        i += 1; // 标识符 `r`，不是原始串前缀。
                    }
                }
                b'\'' => {
                    if i + 1 < n && b[i + 1] == b'\\' {
                        // 转义字符字面量：扫到闭合单引号（`\u{…}` 内的花括号一并跳过）。
                        let mut j = i + 2;
                        while j < n && b[j] != b'\'' {
                            j += 1;
                        }
                        i = if j < n { j + 1 } else { i + 1 };
                    } else if i + 2 < n && b[i + 2] == b'\'' {
                        i += 3; // `'x'`
                    } else {
                        i += 1; // 生命周期标记（`'a` / `'static`）。
                    }
                }
                b'{' => return (i, Some('{')),
                b'}' => return (i, Some('}')),
                b';' => return (i, Some(';')),
                _ => i += 1,
            }
        }
        (n, None)
    }

    /// 从 `open`（指向 `{`）配平到匹配 `}`，**跳过**字符串 / 字符 / 注释。返回匹配 `}` 的字节下标。
    pub(crate) fn brace_match(src: &str, open: usize) -> Option<usize> {
        let mut depth = 0i32;
        let mut i = open;
        loop {
            let (pos, tok) = next_top_token(src, i);
            match tok {
                Some('{') => {
                    depth += 1;
                    i = pos + 1;
                }
                Some('}') => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(pos);
                    }
                    i = pos + 1;
                }
                Some(';') => i = pos + 1,
                _ => return None,
            }
        }
    }

    /// 从 `from` 起定位一个「项」的结束：顶层 `;`（无花括号项）或花括号配平。返回结束后的字节下标。
    fn item_end(src: &str, from: usize) -> usize {
        let (pos, tok) = next_top_token(src, from);
        match tok {
            Some(';') => pos + 1,
            Some('{') => brace_match(src, pos).map(|e| e + 1).unwrap_or(src.len()),
            Some('}') => pos + 1,
            _ => src.len(),
        }
    }

    /// 各行的起始字节下标（按 `\n` 切分，与 `src.split('\n')` 对齐）。
    fn line_starts(src: &str) -> Vec<usize> {
        let mut v = vec![0usize];
        for (i, b) in src.bytes().enumerate() {
            if b == b'\n' {
                v.push(i + 1);
            }
        }
        v
    }

    /// 返回 `src` 的「生产区」逐行 trim 文本：**剔除所有 test-gated 项**
    /// （`mod` / `fn` / 任意项），保留其余全文 —— 不再「截断到首个标记」。
    ///
    /// 宏调用 `prod[i]` 起（`name!(`）的第 `n` 个实参（0 起，去空白、不含逗号）。rustfmt 视长度把调用排成单行或
    /// 一参一行，两种排法同一口径。只按顶层逗号切（实参里的括号 / 方括号不切）。
    pub(crate) fn macro_call_arg(prod: &[String], i: usize, n: usize) -> String {
        let mut call = String::new();
        for l in prod.iter().skip(i).take(16) {
            call.push_str(l);
            if l.ends_with(");") || l.ends_with("}};") {
                break;
            }
        }
        let Some(open) = call.find('(') else {
            return String::new();
        };
        let (mut depth, mut args, mut cur) = (0i32, Vec::new(), String::new());
        for c in call[open + 1..].chars() {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' if depth == 0 => break,
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 0 => {
                    args.push(std::mem::take(&mut cur));
                    continue;
                }
                _ => {}
            }
            cur.push(c);
        }
        args.push(cur);
        args.get(n)
            .map(|a| a.trim().to_string())
            .unwrap_or_default()
    }

    /// 🔴 FIX-GUARD-PRODLINES-422：项体定界改用 [`item_end`]（字符串 / 字符 / 注释感知），
    /// 不再朴素逐字数花括号 ⇒ 测试模块内字符串 / 字符 / 原始串 / 注释里的 `{`/`}` 不再带偏、
    /// 不再把其后生产区整段当测试剔除（`[GUARD-SKIP-BRACE-IN-STRING-382]` 第二次复发的根治）。
    pub(crate) fn prod_lines_excluding_cfg_test(src: &str) -> Vec<String> {
        let parts: Vec<&str> = src.split('\n').collect();
        let starts = line_starts(src);
        let mut out: Vec<String> = Vec::new();
        let mut i = 0usize;
        while i < parts.len() {
            let line = parts[i].strip_suffix('\r').unwrap_or(parts[i]);
            if !is_test_gated_attr(line.trim()) {
                out.push(line.trim().to_string());
                i += 1;
                continue;
            }
            // 跳过属性行本身及其后的文档注释 / 其它属性 / 空行，定位到被 gate 的项首行。
            i += 1;
            while i < parts.len() {
                let t2 = parts[i].strip_suffix('\r').unwrap_or(parts[i]).trim();
                if t2.is_empty()
                    || t2.starts_with("//")
                    || t2.starts_with("#[")
                    || t2.starts_with("#!")
                {
                    i += 1;
                } else {
                    break;
                }
            }
            if i >= parts.len() {
                break;
            }
            // 跳过该项本体（字符串 / 字符 / 注释感知）。
            let end = item_end(src, starts[i]);
            while i < parts.len() && starts[i] < end {
                i += 1;
            }
        }
        out
    }

    #[cfg(test)]
    mod tests {
        use super::prod_lines_excluding_cfg_test;

        fn prod(src: &str) -> Vec<String> {
            prod_lines_excluding_cfg_test(src)
        }

        /// 🔴 422 核心：cfg(test) 模块里含字符串 `"{"`、字符 `'{'`、原始串 `r#"{"#`、注释
        /// `// {`、`/* { */`、生命周期 `'a` ⇒ 其后生产代码行**必须保留**（旧朴素计数会被带偏）。
        #[test]
        fn braces_in_strings_chars_raw_and_comments_do_not_break_skipping() {
            let src = r##"
fn a() {}
#[cfg(test)]
mod t {
    fn x() {
        let s = "{";
        let c = '{';
        let r = r#"{"#;
        // {
        /* { */
        let lt = &'a ();
    }
}
fn b() {}
"##;
            let p = prod(src);
            assert!(p.iter().any(|l| l == "fn a() {}"), "a 应保留");
            assert!(
                p.iter().any(|l| l == "fn b() {}"),
                "跳过 test 模块后 b 必须保留（旧实现会整段剔除）"
            );
            assert!(
                !p.iter().any(|l| l.contains("let s =")),
                "cfg(test) 模块内容应被剔除"
            );
        }

        /// 正常嵌套模块正确剔除 + 无花括号项（`use`/`const`）正确跳过。
        #[test]
        fn nested_mod_and_no_brace_item() {
            let src = "#[cfg(test)]\nmod t {\n    mod inner { fn y() {} }\n}\nfn c() {}\n";
            let p = prod(src);
            assert!(p.iter().any(|l| l == "fn c() {}"));
            assert!(!p.iter().any(|l| l.contains("inner")));

            // `#[cfg(test)]` 只 gate **紧随的一项**：`use` 被剔除，`const`/`fn` 属生产，保留。
            let src2 = "#[cfg(test)]\nuse std::x;\nconst K: i32 = 1;\nfn d() {}\n";
            let p2 = prod(src2);
            assert!(
                !p2.iter().any(|l| l.contains("use std")),
                "紧随 test 属性的 use 应剔除"
            );
            assert!(
                p2.iter().any(|l| l.contains("const K")),
                "只 gate 紧随一项，const 保留"
            );
            assert!(p2.iter().any(|l| l == "fn d() {}"));
        }

        /// 生命周期 `'a`（非字符字面量）不得吞掉后续；转义字符 `'\''` / `'\u{...}'` 内花括号不得带偏。
        #[test]
        fn lifetimes_and_escaped_chars() {
            let src = "#[cfg(test)]\nmod t { fn x<'a>() { let q = '\\''; let u = '\\u{1F600}'; } }\nfn e<'a>() {}\n";
            let p = prod(src);
            assert!(
                p.iter().any(|l| l == "fn e<'a>() {}"),
                "生命周期/转义字符后生产行应保留"
            );
        }
    }
}
