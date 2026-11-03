; Bladr installer (Inno Setup 6).
; Build: ISCC.exe installer\bladr.iss
; CI can override the version: ISCC.exe /DMyAppVersion=1.2.3 installer\bladr.iss

#define MyAppName "Bladr"
#ifndef MyAppVersion
  #define MyAppVersion "0.3.12"
#endif
#define MyAppPublisher "Bladr"
#define MyAppExeName "Bladr.exe"
#define MyAppSource "..\target\release\bladr.exe"

[Setup]
AppId={{8E3B7C2A-6F4D-4A1B-9C7E-2D5F8A0B3C61}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppCopyright=MIT
AppComments=Hurtig billedfremviser til Windows
DefaultDirName={localappdata}\Programs\Bladr
DefaultGroupName=Bladr
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputDir=Output
OutputBaseFilename=Bladr-{#MyAppVersion}-Setup-inner
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName} {#MyAppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
LicenseFile=..\LICENSE
VersionInfoVersion={#MyAppVersion}.0
VersionInfoTextVersion={#MyAppVersion}
VersionInfoProductVersion={#MyAppVersion}
VersionInfoProductName={#MyAppName}
VersionInfoDescription=Bladr installatør
VersionInfoCompany={#MyAppPublisher}

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "da"; MessagesFile: "compiler:Languages\Danish.isl"

[CustomMessages]
da.CreateDesktopIcon=Opret en genvej på skrivebordet
en.CreateDesktopIcon=Create a desktop shortcut
da.FolderContextMenu=Tilføj "Åbn med Bladr" til mapper
en.FolderContextMenu=Add "Open with Bladr" to folders
da.OpenWithBladr=Åbn med Bladr
en.OpenWithBladr=Open with Bladr
da.UninstallShortcut=Afinstaller Bladr
en.UninstallShortcut=Uninstall Bladr

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "foldermenu"; Description: "{cm:FolderContextMenu}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#MyAppSource}"; DestDir: "{app}"; DestName: "{#MyAppExeName}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.da.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Bladr"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\{#MyAppExeName}"
Name: "{group}\{cm:UninstallShortcut}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Bladr"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; Tasks: desktopicon

[Registry]
; Right-click a folder (including a UNC/NAS folder) -> Open with Bladr.
Root: HKCU; Subkey: "Software\Classes\Directory\shell\Bladr"; ValueType: string; ValueName: "MUIVerb"; ValueData: "{cm:OpenWithBladr}"; Tasks: foldermenu; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Directory\shell\Bladr"; ValueType: string; ValueName: "Icon"; ValueData: "{app}\{#MyAppExeName}"; Tasks: foldermenu
Root: HKCU; Subkey: "Software\Classes\Directory\shell\Bladr\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Tasks: foldermenu

; Right-click the empty area inside a folder.
Root: HKCU; Subkey: "Software\Classes\Directory\Background\shell\Bladr"; ValueType: string; ValueName: "MUIVerb"; ValueData: "{cm:OpenWithBladr}"; Tasks: foldermenu; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Directory\Background\shell\Bladr"; ValueType: string; ValueName: "Icon"; ValueData: "{app}\{#MyAppExeName}"; Tasks: foldermenu
Root: HKCU; Subkey: "Software\Classes\Directory\Background\shell\Bladr\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%V"""; Tasks: foldermenu

; Add Bladr to the Windows "Open with" list without changing defaults.
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "Bladr"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Bladr"
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#MyAppExeName},0"
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""

Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".jpg"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".jpeg"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".png"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".gif"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".bmp"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".webp"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".tif"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".tiff"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".ico"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".tga"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".dds"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".hdr"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".pnm"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".qoi"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".cr3"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".cr2"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".nef"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".nrw"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".arw"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".srf"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".sr2"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".dng"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".orf"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".rw2"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".raf"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".pef"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".srw"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".rwl"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".3fr"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".erf"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".mef"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".mos"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".mrw"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".x3f"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".iiq"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".k25"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".kdc"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\Bladr.exe\SupportedTypes"; ValueType: string; ValueName: ".dcr"; ValueData: ""

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,Bladr}"; Flags: nowait postinstall skipifsilent runasoriginaluser
