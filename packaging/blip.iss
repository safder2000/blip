; Blip installer (Inno Setup 6). Built by .github/workflows/release.yml:
;   ISCC /DMyAppVersion=1.2.3 packaging\blip.iss
; Per-user install, no admin needed. Re-running a newer setup upgrades in place,
; which is what the in-app updater does (/VERYSILENT).

#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif

[Setup]
AppId={{6F3D2B1A-8C4E-4B7A-9E21-B11F0E5A7C42}
AppName=Blip
AppVersion={#MyAppVersion}
AppVerName=Blip {#MyAppVersion}
AppPublisher=safder2000
AppPublisherURL=https://github.com/safder2000/blip
AppSupportURL=https://github.com/safder2000/blip/issues
AppUpdatesURL=https://github.com/safder2000/blip/releases
DefaultDirName={localappdata}\Programs\Blip
DisableDirPage=yes
DisableProgramGroupPage=yes
DisableReadyPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=BlipSetup-{#MyAppVersion}
SetupIconFile=..\assets\blip.ico
UninstallDisplayIcon={app}\Blip.exe
UninstallDisplayName=Blip
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=force
RestartApplications=no

[Tasks]
Name: "startup"; Description: "Start Blip when I sign in to Windows"
Name: "desktopicon"; Description: "Desktop shortcut"; Flags: unchecked

[InstallDelete]
Type: filesandordirs; Name: "{app}\_internal"

[Files]
Source: "..\dist\Blip\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\Blip"; Filename: "{app}\Blip.exe"
Name: "{autodesktop}\Blip"; Filename: "{app}\Blip.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "BlipPet"; ValueData: """{app}\Blip.exe"""; Flags: uninsdeletevalue; Tasks: startup

[Run]
Filename: "{app}\Blip.exe"; Description: "Start Blip now"; Flags: nowait postinstall

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/IM Blip.exe /F"; Flags: runhidden; RunOnceId: "StopBlip"
