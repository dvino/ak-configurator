#define MyAppName "AK Configurator"
[Setup]
AppId={{98D90734-FF33-4BCE-9E69-5434767DD69B}
AppName={#MyAppName}
AppVersion={#AppVersion}
VersionInfoVersion={#NumericVersion}
VersionInfoProductVersion={#NumericVersion}
AppPublisher=dvino
DefaultDirName={localappdata}\Programs\AK Configurator
DefaultGroupName=AK Configurator
DisableProgramGroupPage=no
UsePreviousGroup=no
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=commandline
SetupArchitecture=x64
#if MyPlatform == "windows-arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
MinVersion=10.0.22000
#else
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
#endif
UsePreviousAppDir=yes
CloseApplications=no
RestartApplications=no
LicenseFile={#SourceDir}\LICENSE
SetupIconFile=..\assets\ak-configurator.ico
UninstallDisplayIcon={app}\ak-configurator.ico
OutputDir={#OutputDir}
OutputBaseFilename=ak-configurator-{#AppVersion}-{#MyPlatform}-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
#ifdef SigningEnabled
SignTool=akconfig
SignedUninstaller=yes
#endif
[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
[CustomMessages]
english.LegacyFolder=This folder contains a legacy installation. Select another empty folder.
russian.LegacyFolder=В этой папке находится старая установка. Выберите другую пустую папку.
english.NonEmptyFolder=Select an empty folder for AK Configurator.
russian.NonEmptyFolder=Выберите пустую папку для AK Configurator.
[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\assets\ak-configurator.ico"; DestDir: "{app}"; Flags: ignoreversion
[Icons]
Name: "{group}\AK Configurator"; Filename: "{app}\ak-configurator.exe"; WorkingDir: "{app}"; IconFilename: "{app}\ak-configurator.ico"
[Run]
Filename: "{app}\ak-configurator.exe"; Description: "{cm:LaunchProgram,AK Configurator}"; Flags: nowait postinstall skipifsilent unchecked
[Code]
function CheckInstallFolder: String;
var FindRec: TFindRec; Dir, Name, Previous: String; NonEmpty: Boolean;
begin
  Result := '';
  Dir := ExpandConstant('{app}');
  if FileExists(Dir + '\.ak-configurator-owned') then begin
    Result := ExpandConstant('{cm:LegacyFolder}'); exit;
  end;
  if RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{98D90734-FF33-4BCE-9E69-5434767DD69B}_is1', 'InstallLocation', Previous) and
     (CompareText(AddBackslash(Previous), AddBackslash(Dir)) = 0) and FileExists(Dir + '\unins000.exe') then exit;
  NonEmpty := False;
  if FindFirst(Dir + '\*', FindRec) then begin
    try
      repeat
        Name := FindRec.Name;
        if (Name <> '.') and (Name <> '..') then NonEmpty := True;
      until not FindNext(FindRec);
    finally FindClose(FindRec); end;
  end;
  if NonEmpty then begin
    Result := ExpandConstant('{cm:NonEmptyFolder}');
  end;
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var ErrorText: String;
begin
  Result := True;
  if CurPageID = wpSelectDir then begin
    ErrorText := CheckInstallFolder;
    if ErrorText <> '' then begin MsgBox(ErrorText, mbError, MB_OK); Result := False; end;
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := CheckInstallFolder;
end;
