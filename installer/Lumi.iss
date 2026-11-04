[Setup]
AppId={{A7857857-1CE3-4E8E-92DA-A1A45604C94F}
AppName=Lumi
AppVersion={#LumiVersion}
AppPublisher={#LumiAuthor}
AppPublisherURL=https://github.com/dbustosrc/Lumi
AppSupportURL=https://github.com/dbustosrc/Lumi/issues
DefaultDirName={localappdata}\Programs\Lumi
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
MinVersion=10.0
OutputBaseFilename=Lumi-{#LumiVersion}-windows-x64-setup
SetupIconFile={#ProjectDir}\assets\branding\lumi.ico
UninstallDisplayIcon={app}\lumi.exe
CloseApplications=yes
RestartApplications=no
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"

[Files]
Source: "{#LumiBinary}"; DestDir: "{app}"; DestName: "lumi.exe"; Flags: ignoreversion
Source: "{#ProjectDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#ProjectDir}\third-party-licenses\*"; DestDir: "{app}\third-party-licenses"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{userprograms}\Lumi"; Filename: "{app}\lumi.exe"

[Run]
Filename: "{app}\lumi.exe"; Description: "{cm:LaunchProgram,Lumi}"; Flags: nowait postinstall skipifsilent unchecked

[Code]
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  RegisteredExe: String;
  RunKey: String;
begin
  if CurUninstallStep <> usUninstall then
    exit;
  RunKey := 'Software\Microsoft\Windows\CurrentVersion\Run';
  if RegQueryStringValue(HKCU, RunKey, 'Lumi', RegisteredExe) then
    if CompareText(RegisteredExe, '"' + ExpandConstant('{app}\lumi.exe') + '"') = 0 then
      RegDeleteValue(HKCU, RunKey, 'Lumi');
end;
