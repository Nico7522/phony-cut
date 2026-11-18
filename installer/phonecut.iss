; PhoneCut installer (Inno Setup 6). Built by scripts\build-installer.ps1:
;   ISCC /DAppVersion=0.1.0 installer\phonecut.iss
;
; Per-user install, no administrator rights (no UAC prompt):
;   %LOCALAPPDATA%\Programs\PhoneCut
; Layout (the app looks for vendor\ and assets\ next to the executable):
;   phonecut.exe + FFmpeg DLLs, vendor\adb, vendor\scrcpy, vendor\licenses,
;   assets\tutorial, assets\icon

#ifndef AppVersion
  #error AppVersion must be defined (/DAppVersion=x.y.z)
#endif

#define AppName "PhoneCut"
#define AppExe "phonecut.exe"

[Setup]
; Never change AppId: it identifies the installed program for upgrades.
AppId={{74253B47-D1D4-4CF3-90C4-02FAD42C9719}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppName}
AppPublisherURL=https://github.com/Nico7522/phony-cut
AppSupportURL=https://github.com/Nico7522/phony-cut/issues
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
DisableDirPage=auto
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir=..\target\installer
OutputBaseFilename=PhoneCut-Setup-{#AppVersion}
SetupIconFile=..\assets\icon\phonecut.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ShowLanguageDialog=auto
; Detects a running PhoneCut (see platform::SINGLE_INSTANCE_MUTEX).
AppMutex=PhoneCutAppMutex
CloseApplications=yes
RestartApplications=no

[Languages]
; Chosen from the Windows display language; the app itself follows Windows
; too unless the user picks a language in its settings.
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
; FFmpeg (LGPL, shared): the DLLs must sit next to the executable.
Source: "..\vendor\ffmpeg\bin\*.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\vendor\adb\adb.exe"; DestDir: "{app}\vendor\adb"; Flags: ignoreversion
Source: "..\vendor\adb\AdbWinApi.dll"; DestDir: "{app}\vendor\adb"; Flags: ignoreversion
Source: "..\vendor\adb\AdbWinUsbApi.dll"; DestDir: "{app}\vendor\adb"; Flags: ignoreversion
Source: "..\vendor\scrcpy\scrcpy-server"; DestDir: "{app}\vendor\scrcpy"; Flags: ignoreversion
Source: "..\vendor\VERSIONS.md"; DestDir: "{app}\vendor"; Flags: ignoreversion
Source: "..\vendor\licenses\*"; DestDir: "{app}\vendor\licenses"; Flags: ignoreversion
Source: "..\assets\tutorial\*.jpg"; DestDir: "{app}\assets\tutorial"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\assets\tutorial\*.png"; DestDir: "{app}\assets\tutorial"; Flags: ignoreversion skipifsourcedoesntexist
; Optional English pictures (assets\tutorial\en\), used when the app is in English.
Source: "..\assets\tutorial\en\*"; DestDir: "{app}\assets\tutorial\en"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\assets\icon\phonecut.ico"; DestDir: "{app}\assets\icon"; Flags: ignoreversion

[InstallDelete]
; Upgrades: remove pictures that a newer version may have renamed.
Type: filesandordirs; Name: "{app}\assets\tutorial"

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; A lingering adb server would lock vendor\adb\adb.exe.
Filename: "{app}\vendor\adb\adb.exe"; Parameters: "kill-server"; Flags: runhidden waituntilterminated; RunOnceId: "StopAdb"

[UninstallDelete]
; Settings and logs only. Recordings (Videos\PhoneCut) are never touched.
Type: filesandordirs; Name: "{userappdata}\{#AppName}"

[Code]
// Before replacing files during an upgrade, stop the adb server started by
// the previous version (it would keep adb.exe locked).
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Adb: String;
  ResultCode: Integer;
begin
  Adb := ExpandConstant('{app}\vendor\adb\adb.exe');
  if FileExists(Adb) then
    Exec(Adb, 'kill-server', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Result := '';
end;
