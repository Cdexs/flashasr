; flashasr Inno Setup 脚本（FLASH-P2b-04）
; 版本：0.1.0
; 编译：Inno Setup 6.x。产物全部取自 ..\Publish\（显式白名单，不用通配；ggml-cpu-* 变体除外）。
; 安装包不带模型（同 voice-ime）：用户把模型放进 {app}\models\，管理界面「服务」页显示加载结果与模型文件夹。
;
; 🔴 只能按用户安装：数据目录 = 程序所在目录（配置 / 终端配对 / 证书 / 声纹 / 统计 / 日志都写在这里），
;    装到 Program Files 程序就写不了。所以不提供「为所有用户安装」选项（voice-ime 的 OverridesAllowed=dialog 不照搬）。
;    代价：写不了防火墙规则 —— 前台第一次监听端口时 Windows 会弹「允许访问」，勾「专用网络」即可。

#define MyAppName      "flashasr"
#define MyAppVersion   "0.1.0"
#define MyAppPublisher "flashasr"
#define MyAppExeName   "flashasr.exe"
#define MyUiExeName    "flashasr-ui.exe"
#define MyEngineExe    "flashasr-engine.exe"
#define MyAppID        "{{6F1C2E8A-4B7D-4E39-9A21-5C3D8B0E7F42}"

[Setup]
AppId={#MyAppID}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}

DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes

OutputDir=..\dist
OutputBaseFilename=flashasr-Setup-{#MyAppVersion}
#ifexist "..\assets\icons\app.ico"
SetupIconFile=..\assets\icons\app.ico
#endif

Compression=lzma2/ultra64
SolidCompression=yes
CompressionThreads=auto
WizardStyle=modern

; 只按用户装（见文件头）。
PrivilegesRequired=lowest

ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName} {#MyAppVersion}

; Windows 10 / 11
MinVersion=10.0

; 升级时程序在运行：先由 PrepareToInstall 结束进程，不用系统的「关闭应用程序」页。
CloseApplications=no

[Languages]
Name: "chinesesimplified"; MessagesFile: "languages\ChineseSimplified.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
chinesesimplified.AutoStart=开机自动启动识别服务（推荐）
english.AutoStart=Start the recognition service when I sign in (recommended)
chinesesimplified.OpenManager=打开管理界面
english.OpenManager=Open flashasr Manager
chinesesimplified.ManagerShortcut=flashasr 管理
english.ManagerShortcut=flashasr Manager
chinesesimplified.AskPurge=是否同时删除 flashasr 的数据？%n%n包括：终端配对、证书、各终端的声纹、用量统计、配置和日志。%n%n模型文件夹不会删除。选「否」会保留这些数据，重新安装后可继续使用。
english.AskPurge=Also delete flashasr data?%n%nThis includes terminal pairings, the certificate, each terminal's voiceprint, usage statistics, settings and logs.%n%nThe models folder is kept either way. Choose "No" to keep the data for a future reinstall.

[Tasks]
Name: "autostart"; Description: "{cm:AutoStart}"

[Files]
; ---- 程序：前台（托盘 + 对外服务）/ 引擎 / 管理界面 ----
Source: "..\Publish\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\Publish\{#MyEngineExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\Publish\{#MyUiExeName}"; DestDir: "{app}"; Flags: ignoreversion

; ---- 识别运行库（sherpa-onnx / onnxruntime）----
Source: "..\Publish\sherpa-onnx-c-api.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\Publish\sherpa-onnx-cxx-api.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\Publish\onnxruntime.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\Publish\onnxruntime_providers_shared.dll"; DestDir: "{app}"; Flags: ignoreversion

; ---- 离线翻译运行库（CTranslate2）----
Source: "..\Publish\ctranslate2.dll"; DestDir: "{app}"; Flags: ignoreversion

; ---- 精解引擎运行库（llama.cpp Vulkan 版，运行时自选 Vulkan / 最优 CPU 变体）----
Source: "..\Publish\ggml-base.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\Publish\ggml.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\Publish\llama.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\Publish\mtmd.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\Publish\ggml-vulkan.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\Publish\libomp.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\Publish\ggml-cpu-*.dll"; DestDir: "{app}"; Flags: ignoreversion

; ---- VC++ 2015-2022 x64 运行库（随程序放，同 voice-ime）----
Source: "..\Publish\msvcp140.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\Publish\msvcp140_1.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\Publish\vcruntime140.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\Publish\vcruntime140_1.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\Publish\vcomp140.dll"; DestDir: "{app}"; Flags: ignoreversion

; ---- 外置规则表（引擎从程序同级读取）----
Source: "..\Publish\itn-rules.toml"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\Publish\homophone-rules.toml"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\Publish\scene-rules.toml"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\Publish\wordbook-rules.toml"; DestDir: "{app}"; Flags: ignoreversion

; ============================================================================
; 不带（原因）：
;   models\                     : 安装包不带模型
;   config.toml                 : 引擎第一次启动时生成；升级不覆盖
;   asr-*.json / *.der / voiceprints\ / *.sqlite / *.log / local-pairing : 运行时数据
;   cudnn64_9.dll / libiomp5md.dll : 没有任何程序导入（同 voice-ime 结论）
; ============================================================================

[Icons]
Name: "{group}\{cm:ManagerShortcut}"; Filename: "{app}\{#MyUiExeName}"
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"

[Registry]
; 开机自启前台（托盘 + 服务）。
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; \
    ValueType: string; ValueName: "{#MyAppName}"; \
    ValueData: """{app}\{#MyAppExeName}"""; \
    Flags: uninsdeletevalue; Tasks: autostart

[Run]
; 装完总是拉起前台（服务本身；引擎由它拉起，首次运行生成配置 / 证书 / 本机配对文件）。
Filename: "{app}\{#MyAppExeName}"; Flags: nowait runasoriginaluser
Filename: "{app}\{#MyUiExeName}"; Description: "{cm:OpenManager}"; \
    Flags: nowait postinstall skipifsilent runasoriginaluser

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM {#MyUiExeName}"; Flags: runhidden; RunOnceId: "KillUi"
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM {#MyAppExeName}"; Flags: runhidden; RunOnceId: "KillFront"
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM {#MyEngineExe}"; Flags: runhidden; RunOnceId: "KillEngine"

[Code]
procedure KillAll();
var
  Code: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM {#MyUiExeName}', '', SW_HIDE, ewWaitUntilTerminated, Code);
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM {#MyAppExeName}', '', SW_HIDE, ewWaitUntilTerminated, Code);
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM {#MyEngineExe}', '', SW_HIDE, ewWaitUntilTerminated, Code);
end;

// 升级：先结束正在运行的程序，否则文件被占用写不进去。
function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  KillAll();
  Result := '';
end;

procedure DelFiles(const Pattern: String);
begin
  DelTree(ExpandConstant('{app}\') + Pattern, False, True, False);
end;

// 卸载完：问要不要连数据一起删（声纹是生物特征数据，给用户明确的删除选项；静默卸载时保留）。
// 只删数据，不删 models\（用户自己下的、体积大；要删用户自己删）。程序目录删空了才一并移除。
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    if (not UninstallSilent) and
       (MsgBox(CustomMessage('AskPurge'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES) then
    begin
      DelTree(ExpandConstant('{app}\voiceprints'), True, True, True);
      DelFiles('voiceprint.bin');
      DelFiles('asr-*.json');
      DelFiles('asr-*.der');
      DelFiles('*.sqlite');
      DelFiles('*.sqlite-*');
      DelFiles('*.log');
      DelFiles('*.lock');
      DelFiles('config.toml');
      DelTree(ExpandConstant('{userappdata}\flashasr'), True, True, True);
      RemoveDir(ExpandConstant('{app}'));
    end;
  end;
end;
