#ifndef Version
  #define Version "1.0.0"
#endif
#ifndef PayloadDir
  #define PayloadDir "..\artifacts\installer-payload"
#endif
#ifndef OutputDir
  #define OutputDir "..\artifacts\release"
#endif
[Setup]
AppId={{AA3865B7-7E6F-493E-B9BA-52B997735011}
AppName=Easy EQ
AppVersion={#Version}
AppPublisher=Andrew Parkinson
AppPublisherURL=https://github.com/AndyParka/easy-eq
AppSupportURL=https://github.com/AndyParka/easy-eq/issues
DefaultDirName={localappdata}\Programs\Easy EQ
DefaultGroupName=Easy EQ
PrivilegesRequired=lowest
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
MinVersion=10.0.17763
OutputDir={#OutputDir}
OutputBaseFilename=Easy-EQ-{#Version}-windows-x64-setup
SetupIconFile=..\src-tauri\icons\icon.ico
UninstallDisplayIcon={app}\easy-eq.exe
LicenseFile=..\LICENSE
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
DisableWelcomePage=no
CloseApplications=yes
RestartApplications=no
SetupLogging=yes

[Files]
Source: "{#PayloadDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Easy EQ"; Filename: "{app}\easy-eq.exe"
Name: "{autodesktop}\Easy EQ"; Filename: "{app}\easy-eq.exe"; Tasks: desktopicon
Name: "{group}\Set up audio"; Filename: "{sys}\WindowsPowerShell\v1.0\powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\setup-audio.ps1"""; WorkingDir: "{app}"
Name: "{group}\Uninstall Easy EQ"; Filename: "{uninstallexe}"

[Tasks]
Name: desktopicon; Description: "Create a desktop shortcut"; Flags: unchecked
Name: startup; Description: "Start in the tray at sign-in and recover after wake"; Flags: checkedonce
Name: audio; Description: "Set up Windows audio (administrator approval required)"; Flags: checkedonce

[Run]
Filename: "{app}\easy-eq.exe"; Description: "Open Easy EQ"; Flags: nowait postinstall skipifsilent; Check: AudioReady

[UninstallRun]
Filename: "{app}\easy-eq.exe"; Parameters: "--disable-startup"; Flags: runhidden waituntilterminated; RunOnceId: DisableStartup
Filename: "{app}\easy-eq.exe"; Parameters: "--quit"; Flags: runhidden waituntilterminated; RunOnceId: StopApp
Filename: "{sys}\WindowsPowerShell\v1.0\powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\setup-audio.ps1"" -Restore"; Flags: runhidden waituntilterminated; RunOnceId: RestoreAudio

[Code]
var SetupSucceeded: Boolean;
function AudioReady: Boolean;
begin
  Result := SetupSucceeded;
end;
function InitializeSetup: Boolean;
begin
  SetupSucceeded := True;
  Result := True;
end;
procedure InitializeWizard;
begin
  WizardForm.WelcomeLabel2.Caption := 'Easy EQ is a simple Windows equalizer with shared and independent left/right controls.' + #13#10 + #13#10 +
    'Audio setup includes Equalizer APO. Select the playback output you want to equalize when its Device Selector opens. A restart may be needed for a newly configured device.' + #13#10 + #13#10 +
    'If you already use another EQ, disable its filters to avoid stacking adjustments. Existing Equalizer APO settings are preserved.';
end;
function PrepareToInstall(var NeedsRestart: Boolean): String;
var Code: Integer;
begin
  Result := '';
  if FileExists(ExpandConstant('{app}\easy-eq.exe')) then begin
    Exec(ExpandConstant('{app}\easy-eq.exe'), '--quit', '', SW_HIDE, ewWaitUntilTerminated, Code);
    Sleep(1500);
  end;
end;
procedure CurStepChanged(CurStep: TSetupStep);
var Code: Integer;
begin
  if CurStep = ssPostInstall then begin
    if WizardIsTaskSelected('audio') then begin
      if not Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
        '-NoProfile -ExecutionPolicy Bypass -File "' + ExpandConstant('{app}\setup-audio.ps1') + '"',
        ExpandConstant('{app}'), SW_HIDE, ewWaitUntilTerminated, Code) then Code := 1;
      if Code <> 0 then begin
        SetupSucceeded := False;
        if not WizardSilent then MsgBox('Audio setup did not complete. Open "Set up audio" from the Start menu to retry. Details: %LOCALAPPDATA%\Easy EQ\setup-audio.log', mbError, MB_OK);
      end;
    end;
    if WizardIsTaskSelected('startup') then begin
      if not Exec(ExpandConstant('{app}\easy-eq.exe'), '--configure-startup', '', SW_HIDE, ewWaitUntilTerminated, Code) then Code := 1;
      if (Code <> 0) and not WizardSilent then MsgBox('Sign-in recovery could not be registered. Enable it in Easy EQ Settings to retry.', mbError, MB_OK);
    end else begin
      Exec(ExpandConstant('{app}\easy-eq.exe'), '--disable-startup', '', SW_HIDE, ewWaitUntilTerminated, Code);
    end;
  end;
end;
function GetCustomSetupExitCode: Integer;
begin
  if SetupSucceeded then Result := 0 else Result := 1;
end;
