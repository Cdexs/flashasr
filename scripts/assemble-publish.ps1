# assemble-publish.ps1 —— FLASH-P2b-04：把发版产物归集到 Publish\，供 installer\flashasr.iss 打包。
#
# 只复制，不编译、不打包（编译出包等 Gavin 指令）。先手动完成：
#   1. cargo build --release --bins                                   → target\release\flashasr.exe / flashasr-engine.exe
#   2. cd ui; npm ci; npm run build                                   → ui\dist
#   3. cd src-tauri; cargo build --release --features custom-protocol → src-tauri\target\release\flashasr-ui.exe
# 然后：powershell -NoProfile -ExecutionPolicy Bypass -File scripts\assemble-publish.ps1
#
# 运行库取自本仓库 vendor\runtime\（被 .gitignore 忽略，不入库；2026-10-30 从 voice-ime 的 Publish\ 复制来的已验证同一套）。
# 清单 = 下方 $runtime + ggml-cpu-*.dll，与 installer\flashasr.iss 的 [Files] 逐项对应，
# 缺任何一项 ⇒ 报错退出（不静默跳过，免得装出一个跑不起来的包）。各 DLL 来源与更新方法：
#   sherpa-onnx-c-api / sherpa-onnx-cxx-api / onnxruntime / onnxruntime_providers_shared
#       ← vendor\sherpa-onnx\sherpa-onnx-v<版本>-win-x64-shared-MD-Release\lib\（scripts\fetch-sherpa-onnx.ps1 获取；
#         升版本时同步改 .cargo\config.toml 的 SHERPA_ONNX_LIB_DIR）
#   ggml-base / ggml / llama / mtmd / ggml-vulkan / libomp / ggml-cpu-*
#       ← scripts\fetch-llama-runtime.ps1 -Targets vendor\runtime -SkipModel（解压在 vendor\llama-runtime\<tag>\）
#   ctranslate2.dll
#       ← patches\ctranslate2-sys 构建产物（cargo build 后由其 build.rs 复制到 target\<profile>\）
#   msvcp140 / msvcp140_1 / vcruntime140 / vcruntime140_1 / vcomp140
#       ← Visual Studio 的 VC Redist 目录（VC\Redist\MSVC\<版本>\x64\Microsoft.VC14x.CRT 与 .OpenMP）
# 本文件带 UTF-8 BOM（Windows PowerShell 5.1 无 BOM 会按 GBK 读，中文注释会报语法错）。

param(
    [string]$RuntimeDir = (Join-Path (Split-Path $PSScriptRoot -Parent) "vendor\runtime")
)

$ErrorActionPreference = "Stop"
$Root = Split-Path $PSScriptRoot -Parent
$Publish = Join-Path $Root "Publish"

$programs = @(
    @{ From = "target\release\flashasr.exe";                  Name = "flashasr.exe" },
    @{ From = "target\release\flashasr-engine.exe";           Name = "flashasr-engine.exe" },
    @{ From = "src-tauri\target\release\flashasr-ui.exe";     Name = "flashasr-ui.exe" }
)
$runtime = @(
    "sherpa-onnx-c-api.dll", "sherpa-onnx-cxx-api.dll", "onnxruntime.dll", "onnxruntime_providers_shared.dll",
    "ctranslate2.dll",
    "ggml-base.dll", "ggml.dll", "llama.dll", "mtmd.dll", "ggml-vulkan.dll", "libomp.dll",
    "msvcp140.dll", "msvcp140_1.dll", "vcruntime140.dll", "vcruntime140_1.dll", "vcomp140.dll"
)
$rules = @("itn-rules.toml", "homophone-rules.toml", "scene-rules.toml", "wordbook-rules.toml")

$missing = @()
foreach ($p in $programs) { if (-not (Test-Path (Join-Path $Root $p.From))) { $missing += $p.From } }
foreach ($f in $runtime) { if (-not (Test-Path (Join-Path $RuntimeDir $f))) { $missing += "$RuntimeDir\$f" } }
$cpuVariants = @(Get-ChildItem -Path $RuntimeDir -Filter "ggml-cpu-*.dll" -ErrorAction SilentlyContinue)
if ($cpuVariants.Count -eq 0) { $missing += "$RuntimeDir\ggml-cpu-*.dll" }
foreach ($f in $rules) { if (-not (Test-Path (Join-Path $Root $f))) { $missing += $f } }
if ($missing.Count -gt 0) {
    Write-Host "缺少以下文件，未做任何复制：" -ForegroundColor Red
    $missing | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
    exit 1
}

if (-not (Test-Path $Publish)) { New-Item -ItemType Directory -Path $Publish | Out-Null }
foreach ($p in $programs) { Copy-Item (Join-Path $Root $p.From) (Join-Path $Publish $p.Name) -Force }
foreach ($f in $runtime) { Copy-Item (Join-Path $RuntimeDir $f) $Publish -Force }
foreach ($v in $cpuVariants) { Copy-Item $v.FullName $Publish -Force }
foreach ($f in $rules) { Copy-Item (Join-Path $Root $f) $Publish -Force }

$n = $programs.Count + $runtime.Count + $cpuVariants.Count + $rules.Count
Write-Host "已归集 $n 个文件到 $Publish（其中 ggml-cpu 变体 $($cpuVariants.Count) 个）" -ForegroundColor Green
Write-Host "下一步（等出包指令）：用 Inno Setup 6 编译 installer\flashasr.iss"
