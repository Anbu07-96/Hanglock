; Inno Setup 6. Per-user, no elevation, one file plus an uninstaller that removes exactly what it
; added. Chosen over MSIX because MSIX cannot ship a tray app that writes HKCU\...\Run without
; packaging gymnastics, and over WiX because a 3 MB .exe that a person can double-click is the right
; shape for a v0.1 utility.
;
;   iscc /DAppVersion=0.1.0 /DBuildDir=..\target\release scripts\installer.iss

#define MyAppName "Hanglock"
#define MyAppExe "hanglock.exe"
#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef BuildDir
  #define BuildDir "..\target\release"
#endif

[Setup]
AppId={{7B1F4E3A-2C57-4B1E-9E6A-HANG0LOCK001}
AppName={#MyAppName}
AppVersion={#AppVersion}
AppPublisher=Hanglock contributors
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
DisableWelcomePage=no
OutputBaseFilename=Hanglock-{#AppVersion}-x64
OutputDir=..\dist
Compression=lzma2/ultra64
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
WizardStyle=modern
UninstallDisplayIcon={app}\{#MyAppExe}
CloseApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "startup"; Description: "Start {#MyAppName} when I sign in"; GroupDescription: "Options:"
Name: "startup\check"; Description: "(recommended) - a clock you have to remember to open is no use"

[Files]
Source: "{#BuildDir}\{#MyAppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\README.md"; DestDir: "{app}"; DestName: "README.txt"; Flags: ignoreversion skipifsourcedoesntexist

[Run]
Filename: "{app}\{#MyAppExe}"; Description: "Show the clock"; Flags: nowait postinstall skipifsilent

[Registry]
; Only written when the task is ticked; `--background` is the flag the app's own autostart path
; recognises, so a startup launch never pops a console.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Hanglock"; ValueData: """{app}\{#MyAppExe}"" --background"; Tasks: startup; Flags: uninsdeletevalue

[UninstallRun]
; Ask it to leave the tray properly rather than killing it, so it removes its own Run key and the
; notification-area icon instead of leaving a dead one until Explorer restarts.
Filename: "{app}\{#MyAppExe}"; Parameters: "--uninstall"; Flags: runhidden waituntilidle; RunOnceId: "GracefulExit"

[Code]
// Settings live in %APPDATA%\Hanglock and are deliberately NOT removed: a person who reinstalls the
// next version expects their clock back where they left it. The uninstaller says so instead of
// silently deciding.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    MsgBox('Your Hanglock settings were left in %APPDATA%\Hanglock. Delete that folder to start fresh.', mbInformation, MB_OK);
end;
