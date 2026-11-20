; PhonyCut installer (Inno Setup 6). Built by scripts\build-installer.ps1:
;   ISCC /DAppVersion=0.1.0 installer\phonycut.iss
;
; Per-user install, no administrator rights (no UAC prompt):
;   %LOCALAPPDATA%\Programs\PhonyCut
; Layout (the app looks for vendor\ and assets\ next to the executable):
;   PhonyCut.exe + FFmpeg DLLs, vendor\adb, vendor\scrcpy, vendor\licenses,
;   assets\tutorial, assets\icon

#ifndef AppVersion
  #error AppVersion must be defined (/DAppVersion=x.y.z)
#endif

#define AppName "PhonyCut"
#define AppExe "PhonyCut.exe"
; Cargo builds the binary in lower case.
#define BuiltExe "phonycut.exe"

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
; The product was called PhoneCut up to 0.2.0: new shortcuts go to a PhonyCut group
; (the old ones are removed in [InstallDelete]). The install folder of an
; existing installation is kept (moving it would leave the old files behind).
UsePreviousGroup=no
DisableProgramGroupPage=yes
DisableDirPage=auto
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir=..\target\installer
; No version in the name: the website links to releases/latest/download/PhonyCut-Setup-x64.exe
OutputBaseFilename=PhonyCut-Setup-x64
SetupIconFile=..\assets\icon\phonycut.ico
UninstallDisplayIcon={app}\{#AppExe}
; Tells Windows to refresh its icon cache at the end of the install: otherwise a
; shortcut made by an older version keeps showing the old icon after an update.
ChangesAssociations=yes
UninstallDisplayName={#AppName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ShowLanguageDialog=auto
; Detects a running PhonyCut (see platform::SINGLE_INSTANCE_MUTEX), or a running
; version from before the rename (PhoneCut, up to 0.2.0).
AppMutex=PhonyCutAppMutex,PhoneCutAppMutex
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
Source: "..\target\release\{#BuiltExe}"; DestDir: "{app}"; DestName: "{#AppExe}"; Flags: ignoreversion
; FFmpeg (LGPL, shared): the DLLs must sit next to the executable.
Source: "..\vendor\ffmpeg\bin\*.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\vendor\adb\adb.exe"; DestDir: "{app}\vendor\adb"; Flags: ignoreversion
Source: "..\vendor\adb\AdbWinApi.dll"; DestDir: "{app}\vendor\adb"; Flags: ignoreversion
Source: "..\vendor\adb\AdbWinUsbApi.dll"; DestDir: "{app}\vendor\adb"; Flags: ignoreversion
Source: "..\vendor\scrcpy\scrcpy-server"; DestDir: "{app}\vendor\scrcpy"; Flags: ignoreversion
Source: "..\vendor\VERSIONS.md"; DestDir: "{app}\vendor"; Flags: ignoreversion
Source: "..\vendor\licenses\*"; DestDir: "{app}\vendor\licenses"; Flags: ignoreversion
Source: "..\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY_LICENSES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\tutorial\*.jpg"; DestDir: "{app}\assets\tutorial"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\assets\tutorial\*.png"; DestDir: "{app}\assets\tutorial"; Flags: ignoreversion skipifsourcedoesntexist
; Optional English pictures (assets\tutorial\en\), used when the app is in English.
Source: "..\assets\tutorial\en\*"; DestDir: "{app}\assets\tutorial\en"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\assets\icon\phonycut.ico"; DestDir: "{app}\assets\icon"; Flags: ignoreversion

[InstallDelete]
; Upgrades: remove pictures that a newer version may have renamed.
Type: filesandordirs; Name: "{app}\assets\tutorial"
; Upgrades from PhoneCut (up to 0.2.0): old program, icon and shortcuts.
Type: files; Name: "{app}\phonecut.exe"
Type: files; Name: "{app}\assets\icon\phonecut.ico"
Type: filesandordirs; Name: "{autoprograms}\PhoneCut"
Type: files; Name: "{autodesktop}\PhoneCut.lnk"

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; A lingering adb server would lock vendor\adb\adb.exe.
Filename: "{app}\vendor\adb\adb.exe"; Parameters: "kill-server"; Flags: runhidden waituntilterminated; RunOnceId: "StopAdb"

[UninstallDelete]
; Settings and logs only. Recordings (Videos\PhonyCut) are never touched.
Type: filesandordirs; Name: "{userappdata}\{#AppName}"
; Left by a version from before the rename if it was never started again.
Type: filesandordirs; Name: "{userappdata}\PhoneCut"

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
