//! 流式识别的「确认句 + 当前句」状态机与词时间戳（本地实时 [`super::local_stream`] 复用）。
//!
//! flashasr 只做本地实时：原阿里百炼在线流式 ASR 客户端（DashScope WebSocket、热词构造、
//! `run-task` 报文、ASR-058 汇总行等）与连自家识别服务的客户端（`feiyin_remote.rs`）已删除，
//! 只留本地实时仍在用的两项。模块名沿用（与 voice-ime 同名，便于两边对照）。

/// OVERLAY-051-G: 一个词的时间戳信息（纯数据结构）
#[derive(Debug, Clone, PartialEq)]
pub struct WordTiming {
    /// 相对音频起点的开始时间（毫秒）
    pub begin_time: i64,
    /// 相对音频起点的结束时间（毫秒）；缺字段时退化为 begin_time
    pub end_time: i64,
    /// 该词的文本
    pub text: String,
    /// 该词后接的标点；缺字段时退化为空串（拼接时接在 text 之后）
    pub punctuation: String,
}

// ===========================================================================
// StreamingAsrState：增量结果状态机（纯函数，可单测）
// ===========================================================================

/// 流式 ASR 增量结果状态
///
/// 维护已确认句（sentence_end=true）和当前正在变化的句。
/// 设计依据 asr-streaming-pipeline-design-001.md §三
///
/// OVERLAY-051-G: 同时维护与文本同构的 words 累积（confirmed_words / current_words），
/// 保证 `display_words()` 与 `display_text()` 字符级对齐，供时间戳驱动回放使用。
#[derive(Debug, Clone, Default)]
pub struct StreamingAsrState {
    /// 已确认句（sentence_end=true 的句子文本，按顺序）
    confirmed_sentences: Vec<String>,
    /// 当前正在变化的句（最新中间结果的文本）
    current_sentence: String,
    /// 当前句 ID
    current_sentence_id: i64,
    /// OVERLAY-051-G: 已确认句对应的 word timings（与 confirmed_sentences 一一对应）
    confirmed_words: Vec<Vec<WordTiming>>,
    /// OVERLAY-051-G: 当前正在变化句的 word timings（与 current_sentence 同步替换）
    current_words: Vec<WordTiming>,
}

impl StreamingAsrState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 处理 result-generated 事件
    ///
    /// - sentence_end=true：句子确认，加入 confirmed，清空 current
    /// - sentence_end=false 且 sentence_id 相同：同一句的中间修正，替换 current
    /// - sentence_end=false 且 sentence_id 不同：新句开始，更新 current_sentence_id
    ///
    /// OVERLAY-051-G: `words` 参数与 `text` 完全同构处理（end=true push 进 confirmed
    /// 并清空 current；同 id 整体替换 current；新 id 换 id 并替换 current）。
    /// 这保证 `display_words()` 与 `display_text()` 始终字符级对齐。
    pub fn on_result(
        &mut self,
        sentence_id: i64,
        text: &str,
        sentence_end: bool,
        words: &[WordTiming],
    ) {
        if sentence_end {
            self.confirmed_sentences.push(text.to_string());
            self.confirmed_words.push(words.to_vec());
            self.current_sentence.clear();
            self.current_words.clear();
        } else if sentence_id == self.current_sentence_id {
            self.current_sentence = text.to_string();
            self.current_words = words.to_vec();
        } else {
            self.current_sentence_id = sentence_id;
            self.current_sentence = text.to_string();
            self.current_words = words.to_vec();
        }
    }

    /// 合并出完整显示文本（已确认句 + 当前句）
    ///
    /// overlay 侧整段覆盖（Gavin 拍板统一白色，不区分已确认/未确认）
    pub fn display_text(&self) -> String {
        let mut text = self.confirmed_sentences.join("");
        text.push_str(&self.current_sentence);
        text
    }

    /// OVERLAY-051-G: 合并出完整 word timings（与 display_text 字符级对齐）
    ///
    /// 每次 on_result 后调用，返回与 `display_text()` 完全对齐的全量词表。
    /// overlay 侧 `UpdateWordTimings` 直接整体替换，无需合并逻辑。
    pub fn display_words(&self) -> Vec<WordTiming> {
        let mut words: Vec<WordTiming> = Vec::new();
        for sentence_words in &self.confirmed_words {
            words.extend_from_slice(sentence_words);
        }
        words.extend_from_slice(&self.current_words);
        words
    }

    /// 取最终文本（松键后交给 LLM）
    ///
    /// ASR-070: 返回 confirmed + current（与 `display_text()` 同语义）。
    /// 旧实现只返回 `confirmed_sentences.join("")`，依赖一个未经验证的假设
    /// 「finish-task 后服务端会发最后一个 sentence_end=true，届时 current 清空」。
    /// Gavin 2026-08-30 端测实证该假设不成立：松键时最后一段话常未收到
    /// sentence_end=true，current_sentence 的内容被静默丢弃 → 尾部文字丢失。
    /// 修法：final_text 返回 confirmed + current，宁可多给绝不少给
    /// （与 OVERLAY-051-G 契约 5 同向），LLM 后半段会做纠错。
    pub fn final_text(&self) -> String {
        self.display_text()
    }

    /// PUNCT-478（DEC-108）：服务端句间接缝 —— 每个已确认句在 [`Self::final_text`] 里的结束字节位置，
    /// 只取后面还有文字的（最后一句的句末不算接缝）。服务端按停顿断句、在这些位置自动收句末标点，
    /// 由主流程用本地标点模型按前后文重判。
    pub fn sentence_seams(&self) -> Vec<usize> {
        let total = self.final_text().len();
        let mut end = 0usize;
        let mut seams = Vec::new();
        for s in &self.confirmed_sentences {
            end += s.len();
            if end < total && !s.is_empty() {
                seams.push(end);
            }
        }
        seams
    }

    /// 已确认句数
    pub fn confirmed_count(&self) -> usize {
        self.confirmed_sentences.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- StreamingAsrState ---

    #[test]
    fn streaming_state_initial_empty() {
        let state = StreamingAsrState::new();
        assert_eq!(state.display_text(), "");
        assert_eq!(state.final_text(), "");
        assert_eq!(state.confirmed_count(), 0);
    }

    #[test]
    fn streaming_state_intermediate_replaces_current() {
        let mut state = StreamingAsrState::new();
        state.on_result(1, "你好", false, &[]);
        assert_eq!(state.display_text(), "你好");
        state.on_result(1, "你好世界", false, &[]);
        assert_eq!(state.display_text(), "你好世界");
        assert_eq!(state.confirmed_count(), 0);
    }

    /// PUNCT-478（DEC-108）：服务端句间接缝 = 各已确认句在全文中的结束位置（最后一句句末不算）。
    #[test]
    fn p478_sentence_seams_mark_server_sentence_ends() {
        let mut state = StreamingAsrState::new();
        state.on_result(1, "这应该都是文化之间的差异，但是。", true, &[]);
        state.on_result(2, "根本的法则应该都是一样。", true, &[]);
        let text = state.final_text();
        let seams = state.sentence_seams();
        assert_eq!(seams, vec!["这应该都是文化之间的差异，但是。".len()]);
        assert_eq!(&text[seams[0]..], "根本的法则应该都是一样。");
        // 还有未确认的当前句 ⇒ 最后一个已确认句的结尾也是接缝。
        state.on_result(3, "我们", false, &[]);
        assert_eq!(state.sentence_seams().len(), 2);
    }

    #[test]
    fn streaming_state_sentence_end_confirms() {
        let mut state = StreamingAsrState::new();
        state.on_result(1, "你好", false, &[]);
        state.on_result(1, "你好世界", true, &[]);
        assert_eq!(state.confirmed_count(), 1);
        assert_eq!(state.final_text(), "你好世界");
        assert_eq!(state.display_text(), "你好世界");
    }

    #[test]
    fn streaming_state_new_sentence_starts_after_confirm() {
        let mut state = StreamingAsrState::new();
        state.on_result(1, "第一句", true, &[]);
        state.on_result(2, "第二", false, &[]);
        assert_eq!(state.display_text(), "第一句第二");
        // ASR-070: 旧断言 == "第一句" 编码的是「丢弃 current」的缺陷契约，
        // Gavin 2026-08-30 端测实证丢字，契约已改为 confirmed+current。
        assert_eq!(state.final_text(), "第一句第二");
        state.on_result(2, "第二句", true, &[]);
        assert_eq!(state.display_text(), "第一句第二句");
        assert_eq!(state.final_text(), "第一句第二句");
    }

    #[test]
    fn streaming_state_multiple_sentences() {
        let mut state = StreamingAsrState::new();
        state.on_result(1, "A", true, &[]);
        state.on_result(2, "B", true, &[]);
        state.on_result(3, "C", true, &[]);
        assert_eq!(state.final_text(), "ABC");
        assert_eq!(state.confirmed_count(), 3);
    }

    // ASR-070: 钉死新契约的核心场景 —— confirmed 非空且 current 非空时，
    // final_text() 必须返回两者拼接（旧实现只返回 confirmed，丢 current）。
    // 消融自证：若把 final_text 改回 confirmed_sentences.join("")，
    // 这条用例的 assert_eq 会因 "第一句" != "第一句第二" 而变红。
    #[test]
    fn asr_070_final_text_includes_current_sentence() {
        let mut state = StreamingAsrState::new();
        state.on_result(1, "第一句", true, &[]);
        state.on_result(2, "第二", false, &[]);
        // confirmed=["第一句"], current="第二" → final_text 必须含两者
        assert_eq!(state.final_text(), "第一句第二");
        assert_eq!(state.display_text(), "第一句第二");
        // final_text 与 display_text 同语义（ASR-070 修法 A）
        assert_eq!(state.final_text(), state.display_text());
    }

    // ============================================================
    // TEST-SYNC-051G/054B：词表累积同构护栏（StreamingAsrState）
    // words 累积必须与 text 三分支完全同构，否则 display_words() 与
    // display_text() 字符级对不上，时间戳回放按词表放字会与实际文本错位。
    // ============================================================

    fn wt(begin_ms: i64, text: &str) -> WordTiming {
        WordTiming {
            begin_time: begin_ms,
            end_time: begin_ms,
            text: text.to_string(),
            punctuation: String::new(),
        }
    }

    fn wtp(begin_ms: i64, text: &str, punct: &str) -> WordTiming {
        WordTiming {
            begin_time: begin_ms,
            end_time: begin_ms,
            text: text.to_string(),
            punctuation: punct.to_string(),
        }
    }

    /// T9: `sentence_end = true` → 词表进 `confirmed_words`，`current_words` 清空。
    #[test]
    fn sentence_end_moves_words_to_confirmed_and_clears_current() {
        let mut state = StreamingAsrState::new();
        let words = vec![wt(100, "你"), wtp(250, "好", "。")];
        state.on_result(1, "你好。", false, &words);
        state.on_result(1, "你好。", true, &words);
        assert_eq!(state.confirmed_count(), 1);
        assert_eq!(
            state.display_words(),
            words,
            "确认后词表应完整进入 confirmed_words"
        );
        // 新句开始后 current_words 从新句的词表重新累积，不残留旧句
        state.on_result(2, "今天", false, &[wt(300, "今"), wt(400, "天")]);
        assert_eq!(state.display_text(), "你好。今天");
        assert_eq!(
            state.display_words(),
            vec![
                wt(100, "你"),
                wtp(250, "好", "。"),
                wt(300, "今"),
                wt(400, "天")
            ],
            "confirmed_words + 新 current_words 的拼接顺序必须与文本一致"
        );
    }

    /// T10（防重复堆叠核心护栏）：同 `sentence_id` 的中间修正 → 整体替换而非追加。
    /// 连发 3 次同句，词表长度 = 最后一次的长度（3），不是 3 倍（9 或 6）。
    #[test]
    fn same_sentence_id_middle_corrections_replace_not_append() {
        let mut state = StreamingAsrState::new();
        state.on_result(7, "你", false, &[wt(100, "你")]);
        state.on_result(7, "你好", false, &[wt(100, "你"), wt(150, "好")]);
        state.on_result(
            7,
            "你好啊",
            false,
            &[wt(100, "你"), wt(150, "好"), wt(200, "啊")],
        );
        assert_eq!(
            state.display_words().len(),
            3,
            "同句 3 次修正词表长度应为 3，若 append 会堆成 6 或 9"
        );
        assert_eq!(
            state.display_words(),
            vec![wt(100, "你"), wt(150, "好"), wt(200, "啊")],
            "词表应为最后一次修正的内容"
        );
        assert_eq!(state.display_text(), "你好啊");
    }

    /// T11: 新 `sentence_id` → 换句，`current_words` 被新句替换。
    #[test]
    fn new_sentence_id_replaces_current_words() {
        let mut state = StreamingAsrState::new();
        state.on_result(1, "第一句", true, &[wt(100, "第一"), wt(180, "句")]);
        state.on_result(2, "第二。", false, &[wtp(300, "第二", "。")]);
        assert_eq!(state.display_text(), "第一句第二。");
        assert_eq!(
            state.display_words(),
            vec![wt(100, "第一"), wt(180, "句"), wtp(300, "第二", "。")]
        );
    }

    /// T12（同源总校验，最有价值的一条）：`display_words()` 与 `display_text()`
    /// 字符数对齐 —— 跨「两句已确认 + 一句进行中」的组合。
    #[test]
    fn display_words_chars_match_display_text() {
        let mut state = StreamingAsrState::new();
        // 已确认句 1
        state.on_result(1, "今天天气", true, &[wt(100, "今天"), wt(250, "天气")]);
        // 已确认句 2（含标点）
        state.on_result(
            2,
            "很不错。",
            true,
            &[wt(400, "很"), wtp(500, "不错", "。")],
        );
        // 进行中句 3
        state.on_result(3, "明天", false, &[wt(700, "明天")]);

        let text_chars = state.display_text().chars().count();
        let word_chars: usize = state
            .display_words()
            .iter()
            .map(|w| w.text.chars().count() + w.punctuation.chars().count())
            .sum();
        assert_eq!(
            word_chars,
            text_chars,
            "词表字符总数必须等于文本字符数：text={} chars={} words_chars={}",
            state.display_text(),
            text_chars,
            word_chars
        );
        assert_eq!(state.display_text(), "今天天气很不错。明天");
    }
}
