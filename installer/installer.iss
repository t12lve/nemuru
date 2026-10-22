; Installeur Windows de Nemuru
; Les chemins sont relatifs à ce fichier (dossier installer/).

#define MyAppName "Nemuru"
#define MyAppVersion "1.4.0"
#define MyAppPublisher "Nemuru"
#define MyAppExeName "nemuru.exe"

[Setup]
AppId={{E58F492B-3C9A-483A-8C21-6E941589BA30}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\Nemuru
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
LicenseFile=..\LICENSE.txt
OutputDir=..\dist-installer
OutputBaseFilename=Nemuru_Setup_v{#MyAppVersion}
SetupIconFile=app_icon.ico
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64
PrivilegesRequired=lowest

[Languages]
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: isreadme
Source: "..\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\Désinstaller {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Ouvrir {#MyAppName}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: files; Name: "{userappdata}\Nemuru\config.json"
Type: files; Name: "{userappdata}\Nemuru\nemuru.log"
Type: dirifempty; Name: "{userappdata}\Nemuru"
Type: files; Name: "{userappdata}\SommeilDouceur\config.json"
Type: files; Name: "{userappdata}\SommeilDouceur\sommeil.log"
Type: dirifempty; Name: "{userappdata}\SommeilDouceur"
