# 清理清单（2026-10-30，待 Gavin 确认后单独一轮执行）

> 规则：不可逆删除先列清单、单独成轮；目录联接先拆再删；禁 `cargo clean`（会穿透 models 联接）。
> 「入库」= 在 git 里（删了可从历史恢复）；「未入库」= 删了就没了。

## A. 立即可删（P3-D：P3 之后不再需要）—— ✅ 2026-10-30 Gavin「A 删」，已执行（未入库项直接删、入库项 voice-ime `3bd5c5f` git rm；删前确认均非联接、内部无联接；主库 70 个文件完好）

| # | 路径 | 大小 | 入库 | 为什么可以删 |
| --- | --- | --- | --- | --- |
| A1 | `voice-ime\Publish\models\` | 4.27 GB | 未入库 | 模型重复副本；逐文件比对：23 个文件全部在主库 `FlashASR\models` 里且大小一致，副本独有文件 0 个 |
| A2 | `voice-ime\vendor\sherpa-onnx\sherpa-onnx-v1.12.38-win-x64-shared-MD-Release\` + 同名 `.tar.bz2` | 54 MB + 19 MB | 未入库 | 旧版 1.12.38 回滚备份；现用 1.13.8 已迁到 flashasr；输入法已不用 sherpa |
| A3 | `voice-ime\vendor\sherpa-onnx\sherpa-onnx-lib.tar.bz2`、`sherpa-onnx-md.tar.bz2` | 1 KB + 1.2 MB | 入库 | 早期遗留包，无引用 |
| A4 | `voice-ime\vendor\whisper-rs-sys\` | 7.1 MB | 未入库 | 早期 whisper 实验遗留，无引用 |
| A5 | `voice-ime\vendor\cmake\` | 99 KB | 入库 | cmake 补丁只给已删的 ctranslate2 用；flashasr 有自己一份 |
| A6 | `voice-ime\sherpa-onnx-lib\` | 21 MB | 未入库 | 早期 sherpa 库目录，无引用 |
| A7 | `voice-ime\patches\` | 空目录 | — | 补丁已迁到 flashasr |
| A8 | `voice-ime\localtest\p3a-removed-poc\`（5 个 PoC 文件） | 204 KB | 未入库 | 本地引擎 PoC，引擎已删 |
| A9 | `voice-ime\src\main - 副本.rs` | 100 KB | 未入库 | 很早的 main.rs 副本，不参与编译 |

合计约 **4.4 GB**。

## B. 出包构建前清理（Gavin 10-30「最终出包构建前，将debug的输出和缓存全部清理掉」）

| # | 路径 | 大小（10-30 统计） | 说明 |
| --- | --- | --- | --- |
| B1 | `voice-ime\target\`（debug / release / doc / tmp） | 29 GB | 编译输出与缓存；`target\release\models` 联接已拆 |
| B2 | `voice-ime\src-tauri\target\` | 9.2 GB | 设置界面编译输出 |
| B3 | `FlashASR\target\` | 11 GB（不含模型） | ⚠️ 先拆 `target\debug\models` 联接（`& $env:ComSpec /c rmdir`），确认联接已拆再删 |
| B4 | `FlashASR\src-tauri\target\` | 3.2 GB | 管理界面编译输出 |
| B5 | 两仓库 `ui\dist\` | 很小 | 前端构建产物（出包时会重建） |

合计约 **52 GB**。删后首次构建要全量重编（ctranslate2 约 4~5 分钟）。`ui\node_modules`（各约 170 MB）保留（出包要用）。

## 不动

- `FlashASR\models\`（模型唯一一份）。
- `%APPDATA%\voice-ime\models\`（0.1 GB，你现装输入法的数据）。
- `voice-ime\Backup\`（1.4 GB，文档周备份，有自己的保留策略）。
