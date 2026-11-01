; VMPC installer (Inno Setup 6.1+).
;
; Full (offline) installer - bundles vmpc.exe + FFmpeg:
;   ISCC /DMyAppVersion=1.0.0 /DSourceDir=..\dist installer\vmpc.iss
;
; Web installer - tiny setup that downloads vmpc.exe, ffmpeg.exe and ffprobe.exe from the
; GitHub release of the same version and verifies them (SHA-256):
;   ISCC /DWebInstaller /DMyAppVersion=1.0.0 /DRepo=owner/repo
;        /DVmpcSha=... /DFfmpegSha=... /DFfprobeSha=... installer\vmpc.iss
;
; SourceDir must contain vmpc.exe and ffmpeg\ffmpeg.exe + ffmpeg\ffprobe.exe.
; The GitHub workflow (.github/workflows/release.yml) passes all of these automatically.

#define MyAppName "VMPC"
#define MyAppLongName "VMPC - Video Media Player Converter"
#define MyAppExe "vmpc.exe"
#ifndef MyAppVersion
  #define MyAppVersion "1.0.0"
#endif
#ifndef MyAppPublisher
  #define MyAppPublisher "VMPC"
#endif
#ifndef Repo
  #define Repo "OWNER/REPO"
#endif
#ifndef SourceDir
  #define SourceDir "..\dist"
#endif
#ifndef BaseUrl
  #define BaseUrl "https://github.com/" + Repo + "/releases/download/v" + MyAppVersion
#endif
#ifndef VmpcSha
  #define VmpcSha ""
#endif
#ifndef FfmpegSha
  #define FfmpegSha ""
#endif
#ifndef FfprobeSha
  #define FfprobeSha ""
#endif
#ifndef OutputDir
  #define OutputDir "output"
#endif

[Setup]
AppId={{D4FB2B11-FF0A-4E71-ACCD-1DA2BE93FBC2}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppLongName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL=https://github.com/{#Repo}
AppSupportURL=https://github.com/{#Repo}/issues
AppUpdatesURL=https://github.com/{#Repo}/releases
VersionInfoVersion={#MyAppVersion}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
; Installs for the current user without admin rights; "Install for all users" is offered too.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
ChangesAssociations=yes
CloseApplications=yes
SetupIconFile=..\assets\vmpc.ico
UninstallDisplayIcon={app}\{#MyAppExe}
UninstallDisplayName={#MyAppLongName}
WizardStyle=modern
ShowLanguageDialog=auto
WizardSmallImageFile=..\assets\wizard-55.bmp,..\assets\wizard-110.bmp
OutputDir={#OutputDir}
Compression=lzma2/ultra64
SolidCompression=yes
#ifdef WebInstaller
OutputBaseFilename=VMPC-WebSetup-{#MyAppVersion}-x64
#else
OutputBaseFilename=VMPC-Setup-{#MyAppVersion}-x64
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"

[CustomMessages]
english.Associate=Add VMPC to "Open with" for video, audio and image files
german.Associate=VMPC zu "Öffnen mit" für Video-, Audio- und Bilddateien hinzufügen
english.Downloading=Downloading VMPC and the FFmpeg engine...
german.Downloading=VMPC und die FFmpeg-Engine werden heruntergeladen...
english.MediaFile=Media file (VMPC)
german.MediaFile=Mediendatei (VMPC)
english.Preparing=Preparing the media engine...
german.Preparing=Die Medien-Engine wird vorbereitet...

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "associate"; Description: "{cm:Associate}"

[Files]
#ifdef WebInstaller
Source: "{tmp}\vmpc.exe"; DestDir: "{app}"; Flags: external ignoreversion
Source: "{tmp}\ffmpeg.exe"; DestDir: "{app}\ffmpeg"; Flags: external ignoreversion
Source: "{tmp}\ffprobe.exe"; DestDir: "{app}\ffmpeg"; Flags: external ignoreversion
#else
Source: "{#SourceDir}\vmpc.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\ffmpeg\ffmpeg.exe"; DestDir: "{app}\ffmpeg"; Flags: ignoreversion
Source: "{#SourceDir}\ffmpeg\ffprobe.exe"; DestDir: "{app}\ffmpeg"; Flags: ignoreversion
#endif
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"; Comment: "{#MyAppLongName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"; Tasks: desktopicon

[Registry]
; "Open with" registration. Windows 10/11 let the user pick the default app themselves.
Root: HKA; Subkey: "Software\Classes\VMPC.Media"; ValueType: string; ValueName: ""; ValueData: "{cm:MediaFile}"; Flags: uninsdeletekey; Tasks: associate
Root: HKA; Subkey: "Software\Classes\VMPC.Media\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#MyAppExe},0"; Tasks: associate
Root: HKA; Subkey: "Software\Classes\VMPC.Media\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExe}"" ""%1"""; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#MyAppName}"; Flags: uninsdeletekey; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExe}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExe}"" ""%1"""; Tasks: associate
#define Assoc(str Ext) \
  "Root: HKA; Subkey: ""Software\Classes\." + Ext + "\OpenWithProgids""; ValueType: string; ValueName: ""VMPC.Media""; ValueData: """"; Flags: uninsdeletevalue; Tasks: associate" + NewLine + \
  "Root: HKA; Subkey: ""Software\Classes\Applications\" + MyAppExe + "\SupportedTypes""; ValueType: string; ValueName: ""." + Ext + """; ValueData: """"; Tasks: associate" + NewLine
{#Assoc("mp4")}{#Assoc("m4v")}{#Assoc("mkv")}{#Assoc("webm")}{#Assoc("avi")}{#Assoc("mov")}
{#Assoc("wmv")}{#Assoc("flv")}{#Assoc("mpg")}{#Assoc("mpeg")}{#Assoc("ts")}{#Assoc("m2ts")}
{#Assoc("3gp")}{#Assoc("ogv")}{#Assoc("mp3")}{#Assoc("flac")}{#Assoc("wav")}{#Assoc("ogg")}
{#Assoc("opus")}{#Assoc("m4a")}{#Assoc("aac")}{#Assoc("wma")}{#Assoc("aiff")}{#Assoc("gif")}

[Run]
; Start the engine once, hidden: Windows scans a freshly installed executable on its first
; run (~10 s for the 170 MB FFmpeg binaries). Doing it here keeps the first file open instant.
Filename: "{app}\ffmpeg\ffprobe.exe"; Parameters: "-v quiet -version"; StatusMsg: "{cm:Preparing}"; Flags: runhidden waituntilterminated
Filename: "{app}\ffmpeg\ffmpeg.exe"; Parameters: "-v quiet -version"; StatusMsg: "{cm:Preparing}"; Flags: runhidden waituntilterminated
Filename: "{app}\{#MyAppExe}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall skipifsilent
; In-app auto update: the app runs this setup with /SILENT /RELAUNCH=1, so start it again.
Filename: "{app}\{#MyAppExe}"; Flags: nowait runasoriginaluser; Check: ShouldRelaunch

[Code]
function ShouldRelaunch: Boolean;
begin
  Result := ExpandConstant('{param:relaunch|0}') = '1';
end;

#ifdef WebInstaller
[Code]
var
  DownloadPage: TDownloadWizardPage;
  Downloaded: Boolean;

function OnDownloadProgress(const Url, FileName: String; const Progress, ProgressMax: Int64): Boolean;
begin
  Result := True;
end;

procedure InitializeWizard;
begin
  DownloadPage := CreateDownloadPage(SetupMessage(msgWizardPreparing), CustomMessage('Downloading'), @OnDownloadProgress);
end;

{ Interactive install: download with a progress page after "Install" is clicked. }
function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if (CurPageID = wpReady) and not Downloaded then
  begin
    DownloadPage.Clear;
    DownloadPage.Add('{#BaseUrl}/vmpc.exe', 'vmpc.exe', '{#VmpcSha}');
    DownloadPage.Add('{#BaseUrl}/ffmpeg.exe', 'ffmpeg.exe', '{#FfmpegSha}');
    DownloadPage.Add('{#BaseUrl}/ffprobe.exe', 'ffprobe.exe', '{#FfprobeSha}');
    DownloadPage.Show;
    try
      try
        DownloadPage.Download;
        Downloaded := True;
      except
        if not DownloadPage.AbortedByUser then
          SuppressibleMsgBox(AddPeriod(GetExceptionMessage), mbCriticalError, MB_OK, IDOK);
        Result := False;
      end;
    finally
      DownloadPage.Hide;
    end;
  end;
end;

{ Silent install (/SILENT, /VERYSILENT) skips the wizard pages, so download here instead. }
function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if not Downloaded then
  try
    DownloadTemporaryFile('{#BaseUrl}/vmpc.exe', 'vmpc.exe', '{#VmpcSha}', @OnDownloadProgress);
    DownloadTemporaryFile('{#BaseUrl}/ffmpeg.exe', 'ffmpeg.exe', '{#FfmpegSha}', @OnDownloadProgress);
    DownloadTemporaryFile('{#BaseUrl}/ffprobe.exe', 'ffprobe.exe', '{#FfprobeSha}', @OnDownloadProgress);
    Downloaded := True;
  except
    Result := GetExceptionMessage;
  end;
end;
#endif
