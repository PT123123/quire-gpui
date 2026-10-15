; Optional per-user Windows installer. Build the portable zip with
; install/build-release.ps1; pass -BuildInstaller to also compile this file.

#define AppExe "..\target\release\quire-gpui.exe"
#define AppVersion Copy(GetFileVersion(AppExe), 1, RPos(".", GetFileVersion(AppExe)) - 1)

[Setup]
AppId={{E8318F69-823B-4F7F-A386-01559641E134}
AppName=Quire GPUI
AppVersion={#AppVersion}
AppVerName=Quire GPUI {#AppVersion}
AppPublisher=Quire
DefaultDirName={localappdata}\Programs\Quire GPUI
DisableProgramGroupPage=yes
OutputDir=..\dist
OutputBaseFilename=Quire-GPUI-{#AppVersion}-windows-x64-setup
SetupIconFile=..\assets\quire.ico
UninstallDisplayIcon={app}\quire-gpui.exe
UninstallDisplayName=Quire GPUI
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes

[Files]
Source: "{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD-PARTY-NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional icons:"; Flags: unchecked

[Icons]
Name: "{autoprograms}\Quire GPUI"; Filename: "{app}\quire-gpui.exe"; IconFilename: "{app}\quire-gpui.exe"
Name: "{autodesktop}\Quire GPUI"; Filename: "{app}\quire-gpui.exe"; IconFilename: "{app}\quire-gpui.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\quire-gpui.exe"; Description: "Launch Quire GPUI"; Flags: nowait postinstall skipifsilent
