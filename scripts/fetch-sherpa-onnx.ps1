# fetch-sherpa-onnx.ps1
#
# 一键获取 Windows 版 sherpa-onnx 预编译共享库。源自 voice-ime（2026-10-30 迁入本仓库自带）。
#
# 背景：vendor/sherpa-onnx/ 下的解压目录被 .gitignore 排除（/vendor/sherpa-onnx/*-Release/），
# 因此全新 git checkout 若没有解压目录就无法构建。本脚本供 Windows 开发者在 clone 后运行，
# 自动下载并解压对应版本的预编译包，使 SHERPA_ONNX_LIB_DIR（.cargo/config.toml）指向的路径存在。
#
# 官方 1.13.x 的发布包名带 `-lib` 后缀（sherpa-onnx-v1.13.8-win-x64-shared-MD-Release-lib.tar.bz2，
# 解压目录同样带 `-lib`）；本脚本落盘时去掉 `-lib`，与 .cargo/config.toml 的目录名一致。
#
# 用法（在仓库根或任意目录均可，相对路径按仓库根解析）：
#   powershell -ExecutionPolicy Bypass -File scripts\fetch-sherpa-onnx.ps1 [-Version "1.13.8"] [-OutDir "vendor\sherpa-onnx"]
#
# 幂等：如果目标目录已存在，脚本会直接跳过。
# 本文件带 UTF-8 BOM（Windows PowerShell 5.1 无 BOM 会按 GBK 读，中文会乱码 / 报语法错）。

param(
    [string]$Version = "1.13.8",
    [string]$OutDir = "vendor\sherpa-onnx",
    [string]$Flavor = "win-x64-shared-MD-Release",
    # 官方资产名后缀：1.13.x 为 "-lib"，1.12.x 为 ""
    [string]$AssetSuffix = "-lib"
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
if (-not [IO.Path]::IsPathRooted($OutDir)) { $OutDir = Join-Path $Root $OutDir }

$packageName = "sherpa-onnx-v$Version-$Flavor"
$remoteName = "$packageName$AssetSuffix"
$url = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v$Version/$remoteName.tar.bz2"
$outFile = Join-Path $OutDir "$packageName.tar.bz2"
$expectedDir = Join-Path $OutDir $packageName

# 幂等：目标目录已存在则跳过
if (Test-Path $expectedDir) {
    Write-Host "sherpa-onnx 已存在：$expectedDir"
    exit 0
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

if (-not (Test-Path $outFile)) {
    Write-Host "正在下载 sherpa-onnx v$Version ($Flavor) ..."
    Invoke-WebRequest -Uri $url -OutFile $outFile
}

Write-Host "正在解压 $outFile ..."
# Windows 10/11 自带 tar 支持 bzip2
tar -xjf $outFile -C $OutDir
if ($LASTEXITCODE -ne 0) { throw "解压失败：$outFile" }
if ($AssetSuffix -ne "" -and (Test-Path (Join-Path $OutDir $remoteName))) {
    Rename-Item (Join-Path $OutDir $remoteName) $packageName
}

Write-Host "sherpa-onnx 已解压到 $expectedDir"
