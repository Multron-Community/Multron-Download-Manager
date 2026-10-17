#define AppName "Multron Download Manager"
#define AppExe "multron-download-manager.exe"
#define AppPublisher "Multron Community"
#define AppUrl "https://github.com/Multron-Community/Multron-Download-Manager"
#define AppDataId "com.multron.downloadmanager"

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\..\src-tauri\target\release\multron-download-manager.exe"
#endif
#ifndef OutputDir
  #define OutputDir "..\..\dist"
#endif

[Setup]
AppId={{7E3C2B1A-4D5F-4B8E-9C1A-6F2D3E4B5A61}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}/issues
AppUpdatesURL={#AppUrl}/releases
AppCopyright=© {#AppPublisher}
VersionInfoVersion={#AppVersion}
VersionInfoCompany={#AppPublisher}
VersionInfoDescription={#AppName} Setup
VersionInfoProductName={#AppName}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
LicenseFile=..\..\LICENSE
OutputDir={#OutputDir}
OutputBaseFilename=Multron-Download-Manager-{#AppVersion}-x64-Setup
SetupIconFile=..\..\src-tauri\icons\icon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ShowLanguageDialog=no
LanguageDetectionMethod=uilanguage
CloseApplications=force
RestartApplications=no
ChangesAssociations=yes
MinVersion=10.0

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"

[CustomMessages]
AssocGroup=File associations:
AssocTorrent=Open .torrent files with Multron Download Manager
AssocMagnet=Open magnet: links with Multron Download Manager
WebView2=Installing Microsoft Edge WebView2 Runtime...
ExtensionFolder=Browser extension folder
turkish.AssocGroup=Dosya ilişkilendirmeleri:
turkish.AssocTorrent=.torrent dosyalarını Multron Download Manager ile aç
turkish.AssocMagnet=magnet: bağlantılarını Multron Download Manager ile aç
turkish.WebView2=Microsoft Edge WebView2 Runtime kuruluyor...
turkish.ExtensionFolder=Tarayıcı eklentisi klasörü
german.AssocGroup=Dateizuordnungen:
german.AssocTorrent=.torrent-Dateien mit Multron Download Manager öffnen
german.AssocMagnet=magnet:-Links mit Multron Download Manager öffnen
german.WebView2=Microsoft Edge WebView2 Runtime wird installiert...
german.ExtensionFolder=Ordner der Browsererweiterung
spanish.AssocGroup=Asociaciones de archivos:
spanish.AssocTorrent=Abrir archivos .torrent con Multron Download Manager
spanish.AssocMagnet=Abrir enlaces magnet: con Multron Download Manager
spanish.WebView2=Instalando Microsoft Edge WebView2 Runtime...
spanish.ExtensionFolder=Carpeta de la extensión del navegador
french.AssocGroup=Associations de fichiers :
french.AssocTorrent=Ouvrir les fichiers .torrent avec Multron Download Manager
french.AssocMagnet=Ouvrir les liens magnet: avec Multron Download Manager
french.WebView2=Installation de Microsoft Edge WebView2 Runtime...
french.ExtensionFolder=Dossier de l'extension du navigateur
italian.AssocGroup=Associazioni dei file:
italian.AssocTorrent=Apri i file .torrent con Multron Download Manager
italian.AssocMagnet=Apri i link magnet: con Multron Download Manager
italian.WebView2=Installazione di Microsoft Edge WebView2 Runtime...
italian.ExtensionFolder=Cartella dell'estensione del browser
brazilianportuguese.AssocGroup=Associações de arquivos:
brazilianportuguese.AssocTorrent=Abrir arquivos .torrent com o Multron Download Manager
brazilianportuguese.AssocMagnet=Abrir links magnet: com o Multron Download Manager
brazilianportuguese.WebView2=Instalando o Microsoft Edge WebView2 Runtime...
brazilianportuguese.ExtensionFolder=Pasta da extensão do navegador
russian.AssocGroup=Сопоставление файлов:
russian.AssocTorrent=Открывать файлы .torrent в Multron Download Manager
russian.AssocMagnet=Открывать ссылки magnet: в Multron Download Manager
russian.WebView2=Установка Microsoft Edge WebView2 Runtime...
russian.ExtensionFolder=Папка расширения браузера
japanese.AssocGroup=ファイルの関連付け:
japanese.AssocTorrent=.torrent ファイルを Multron Download Manager で開く
japanese.AssocMagnet=magnet: リンクを Multron Download Manager で開く
japanese.WebView2=Microsoft Edge WebView2 Runtime をインストールしています...
japanese.ExtensionFolder=ブラウザー拡張機能のフォルダー
korean.AssocGroup=파일 연결:
korean.AssocTorrent=.torrent 파일을 Multron Download Manager로 열기
korean.AssocMagnet=magnet: 링크를 Multron Download Manager로 열기
korean.WebView2=Microsoft Edge WebView2 Runtime 설치 중...
korean.ExtensionFolder=브라우저 확장 프로그램 폴더

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: checkedonce
Name: "torrentassoc"; Description: "{cm:AssocTorrent}"; GroupDescription: "{cm:AssocGroup}"
Name: "magnetassoc"; Description: "{cm:AssocMagnet}"; GroupDescription: "{cm:AssocGroup}"; Flags: unchecked

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "{#AppExe}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\extension\*"; DestDir: "{app}\browser-extension"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\..\extension-build\firefox\*"; DestDir: "{app}\browser-extension-firefox"; Flags: ignoreversion recursesubdirs createallsubdirs
#ifdef WebView2Bootstrapper
Source: "{#WebView2Bootstrapper}"; DestDir: "{tmp}"; DestName: "MicrosoftEdgeWebview2Setup.exe"; Flags: deleteafterinstall; Check: NeedsWebView2
#endif

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{group}\{cm:ExtensionFolder}"; Filename: "{app}\browser-extension"
Name: "{group}\{cm:UninstallProgram,{#AppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\Classes\.torrent\OpenWithProgids"; ValueType: string; ValueName: "Multron.Torrent"; ValueData: ""; Flags: uninsdeletevalue; Tasks: torrentassoc
Root: HKA; Subkey: "Software\Classes\.torrent"; ValueType: string; ValueName: ""; ValueData: "Multron.Torrent"; Flags: uninsdeletevalue; Tasks: torrentassoc
Root: HKA; Subkey: "Software\Classes\Multron.Torrent"; ValueType: string; ValueName: ""; ValueData: "BitTorrent file"; Flags: uninsdeletekey; Tasks: torrentassoc
Root: HKA; Subkey: "Software\Classes\Multron.Torrent\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExe},0"; Tasks: torrentassoc
Root: HKA; Subkey: "Software\Classes\Multron.Torrent\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""; Tasks: torrentassoc
Root: HKA; Subkey: "Software\Classes\magnet"; ValueType: string; ValueName: ""; ValueData: "URL:Magnet link"; Flags: uninsdeletekey; Tasks: magnetassoc
Root: HKA; Subkey: "Software\Classes\magnet"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""; Tasks: magnetassoc
Root: HKA; Subkey: "Software\Classes\magnet\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExe},0"; Tasks: magnetassoc
Root: HKA; Subkey: "Software\Classes\magnet\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""; Tasks: magnetassoc
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "{#AppName}"; Flags: uninsdeletevalue dontcreatekey
Root: HKCU; Subkey: "Software\Google\Chrome\NativeMessagingHosts\com.multron.downloadmanager"; Flags: uninsdeletekey dontcreatekey
Root: HKCU; Subkey: "Software\Chromium\NativeMessagingHosts\com.multron.downloadmanager"; Flags: uninsdeletekey dontcreatekey
Root: HKCU; Subkey: "Software\Microsoft\Edge\NativeMessagingHosts\com.multron.downloadmanager"; Flags: uninsdeletekey dontcreatekey
Root: HKCU; Subkey: "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.multron.downloadmanager"; Flags: uninsdeletekey dontcreatekey
Root: HKCU; Subkey: "Software\Vivaldi\NativeMessagingHosts\com.multron.downloadmanager"; Flags: uninsdeletekey dontcreatekey
Root: HKCU; Subkey: "Software\Mozilla\NativeMessagingHosts\com.multron.downloadmanager"; Flags: uninsdeletekey dontcreatekey

[Run]
#ifdef WebView2Bootstrapper
Filename: "{tmp}\MicrosoftEdgeWebview2Setup.exe"; Parameters: "/silent /install"; StatusMsg: "{cm:WebView2}"; Check: NeedsWebView2; Flags: waituntilterminated
#endif
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent shellexec
Filename: "{app}\{#AppExe}"; Flags: nowait; Check: WizardSilent and RelaunchRequested

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/IM {#AppExe} /F"; Flags: runhidden; RunOnceId: "StopMultron"

[UninstallDelete]
Type: filesandordirs; Name: "{app}\browser-extension"
Type: filesandordirs; Name: "{app}\browser-extension-firefox"
Type: filesandordirs; Name: "{userappdata}\{#AppDataId}"
Type: filesandordirs; Name: "{localappdata}\{#AppDataId}"
Type: dirifempty; Name: "{app}"

[Code]
function RelaunchRequested: Boolean;
begin
  Result := ExpandConstant('{param:relaunch|0}') = '1';
end;

const
  WebView2Key = 'Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';

function HasWebView2(RootKey: Integer; SubKey: String): Boolean;
var
  Version: String;
begin
  Result := RegQueryStringValue(RootKey, SubKey, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0');
end;

function NeedsWebView2: Boolean;
begin
  Result := not (HasWebView2(HKLM, 'SOFTWARE\WOW6432Node\' + WebView2Key)
    or HasWebView2(HKLM, 'SOFTWARE\' + WebView2Key)
    or HasWebView2(HKCU, 'Software\' + WebView2Key));
end;
