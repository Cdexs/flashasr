//! MODELS-PACK（Gavin 10-30）：模型文件清单与「缺了哪些」检测。
//!
//! 安装包不带模型（约 4.3GB），模型包另放在 GitHub Release（7z 分卷）。管理界面用这里的清单判断模型是否放好，
//! 缺了就显示下载地址与放置步骤。清单 = 模型包内容 = 引擎加载的文件（文件夹名 / 文件名改动须三处同步）。

use std::path::Path;

/// 模型包下载页（GitHub Release，三个 7z 分卷 + 校验值）。管理界面显示、可点击 / 复制。
pub const MODELS_DOWNLOAD_URL: &str = "https://github.com/Cdexs/flashasr/releases/tag/models-v1";

/// 模型包里的文件夹与每个文件夹里引擎必须读到的文件（相对模型目录）。
pub const MODEL_FILES: &[(&str, &[&str])] = &[
    (
        "qwen3-asr-1.7b-gguf",
        &[
            "Qwen3-ASR-1.7B-Q8_0.gguf",
            "mmproj-Qwen3-ASR-1.7B-Q8_0.gguf",
        ],
    ),
    (
        "qwen3-forcedaligner-0.6b-gguf",
        &[
            "aligner-backbone-q8_0.gguf",
            "aligner-head.bin",
            "aligner-mmproj-q8_0.gguf",
        ],
    ),
    (
        "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17",
        &["model.int8.onnx", "tokens.txt"],
    ),
    ("punct-ct-transformer-zh", &["model.onnx"]),
    (
        "punct-cap-seg-47lang-int8",
        &["model.int8.onnx", "spe_unigram_64k_lowercase_47lang.model"],
    ),
    (
        "speaker-campplus-zh-en",
        &["3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx"],
    ),
    ("silero-vad", &["silero_vad.onnx"]),
    ("nllb-200-distilled-600M-ct2-int8", &["model.bin"]),
];

/// 缺文件的模型文件夹名（按清单顺序；空 = 都在）。只看文件在不在、不读内容（界面每秒刷新也不费事）。
pub fn missing_models(model_dir: &Path) -> Vec<String> {
    MODEL_FILES
        .iter()
        .filter(|(dir, files)| files.iter().any(|f| !model_dir.join(dir).join(f).is_file()))
        .map(|(dir, _)| dir.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_dir_misses_everything_and_full_dir_misses_nothing() {
        let tmp = std::env::temp_dir().join(format!("flashasr-models-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        assert_eq!(missing_models(&tmp).len(), MODEL_FILES.len());
        for (dir, files) in MODEL_FILES {
            std::fs::create_dir_all(tmp.join(dir)).unwrap();
            for f in *files {
                std::fs::write(tmp.join(dir).join(f), b"x").unwrap();
            }
        }
        assert!(missing_models(&tmp).is_empty());
        std::fs::remove_file(tmp.join("silero-vad").join("silero_vad.onnx")).unwrap();
        assert_eq!(missing_models(&tmp), vec!["silero-vad".to_string()]);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
