import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { appVersion, root } from './lib.mjs';

const REPO = 'https://github.com/Multron-Community/Multron-Download-Manager';
const RAW = 'https://raw.githubusercontent.com/Multron-Community/Multron-Download-Manager/main';
const EMAIL = 'winball501@gmail.com';
export const PAD_NAME = 'multron-download-manager.xml';

const DESCRIPTIONS = {
  Char_Desc_45: 'Fast download manager for files and videos',
  Char_Desc_80: 'Fast multi-connection download manager for files, videos and torrents',
  Char_Desc_250:
    'Fast multi-connection download manager for Windows, macOS and Linux. Downloads files, videos and music from 1,800+ sites, cloud drives and torrents, with a browser extension, SHA-256 checks and signed automatic updates.',
  Char_Desc_450:
    'Multron Download Manager is a fast, modern download manager for Windows, macOS and Linux. It splits every file across up to 64 connections and writes straight to disk, resumes safely and survives VPN and network changes. It downloads videos and music from 1,800+ sites, Google Drive and MEGA files and torrents, and its extension for 35+ browsers sends downloads to it automatically. SHA-256 checks, antivirus scans and 12 languages.',
  Char_Desc_2000: [
    'Multron Download Manager is a fast, modern download manager for Windows, macOS and Linux, for direct links, videos, cloud-drive files and torrents in one app.',
    'Speed: every file is split across 8 connections by default (1 to 64, changeable while the download runs). Connections that finish early take over the slowest parts, and data is written straight into the final file, so there is no merge step. Disk space is checked before a download starts, and global, per-download and scheduled speed limits are available.',
    'Reliability: downloads resume after a pause, crash or restart, and the app notices when a file changed on the server. Turning a VPN on or off or losing the connection does not fail a download; it waits for the network and continues.',
    'Videos and music: YouTube, Reddit, Instagram, X, TikTok, Facebook, Vimeo, Twitch, SoundCloud and 1,800+ other sites through yt-dlp, in 4K, 1080p, 720p or as MP3. yt-dlp and FFmpeg are installed and updated automatically. Google Drive, MEGA, Dropbox, MediaFire and Pixeldrain links are downloaded directly.',
    'Torrents: magnet links and .torrent files, file selection, play while downloading, DHT, uTP, UPnP, trackers, seeding rules, RSS feeds, a watch folder, torrent creation, SOCKS5 proxy and IP filters.',
    'Browser extension: for Chrome, Edge, Brave, Opera, Vivaldi, Firefox and 35+ browsers. It takes over browser downloads with their cookies, adds a download button to videos, can download everything on a page and starts the app when it is closed.',
    'Security: SHA-256 verification, antivirus scans of finished files on Windows, the SmartScreen mark and file-name spoofing protection. Updates are signed and verified before they are installed.',
    'Also: queue control, bulk actions, categories, tray icon, taskbar progress, light and dark themes and 12 languages.'
  ].join('\n\n')
};
const LIMITS = { Char_Desc_45: 45, Char_Desc_80: 80, Char_Desc_250: 250, Char_Desc_450: 450, Char_Desc_2000: 2000 };
const KEYWORDS =
  'download manager,downloader,download accelerator,video downloader,youtube downloader,torrent client,bittorrent,magnet,yt-dlp,multi-connection,resume downloads,browser extension,google drive,mega,sha-256';
const LICENSE_URL = `${REPO}/blob/main/LICENSE`;

function escape(value) {
  return String(value).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

function plain(markdown) {
  return markdown
    .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1')
    .replace(/[*`_]/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}

export function changeInfo(version) {
  const file = path.join(root, 'release-notes', `v${version}.md`);
  if (!fs.existsSync(file)) return `Multron Download Manager ${version}.`;
  const blocks = fs.readFileSync(file, 'utf8').split(/\n\s*\n/).map((b) => b.trim());
  const firstSection = blocks.findIndex((b) => b.startsWith('### '));
  const intro = blocks
    .slice(0, firstSection === -1 ? blocks.length : firstSection)
    .find((b) => b && !b.startsWith('#') && !b.startsWith('-') && !b.startsWith('!'));
  const sections = [];
  for (let i = 0; i < blocks.length; i++) {
    if (!blocks[i].startsWith('### ')) continue;
    const heading = blocks[i].split('\n')[0].slice(4).trim();
    if (/^updating$/i.test(heading)) continue;
    const labels = [];
    for (let j = i + 1; j < blocks.length && !blocks[j].startsWith('#'); j++) {
      for (const m of blocks[j].matchAll(/^- \*\*([^*]+?)\*\*/gm)) labels.push(m[1].replace(/[.:]$/, '').trim().toLowerCase());
    }
    sections.push(labels.length ? `${heading}: ${labels.join(', ')}.` : `${heading}.`);
  }
  const text = plain([intro, ...sections].filter(Boolean).join(' '));
  return text.length > 1000 ? text.slice(0, 997).replace(/\s+\S*$/, '') + '…' : text;
}

export function padXml({ version, date, size }) {
  for (const [key, limit] of Object.entries(LIMITS)) {
    if (DESCRIPTIONS[key].length > limit) throw new Error(`${key} is ${DESCRIPTIONS[key].length} characters, limit ${limit}`);
  }
  if (KEYWORDS.length > 250) throw new Error('keywords are longer than 250 characters');
  const [year, month, day] = date.split('-');
  const setup = `Multron-Download-Manager-${version}-x64-Setup.exe`;
  const tag = (name, value = '') => `<${name}>${escape(value)}</${name}>`;
  const block = (name, ...children) => `<${name}>\n${children.join('\n')}\n</${name}>`;
  return `<?xml version="1.0" encoding="UTF-8"?>\n${block(
    'XML_DIZ_INFO',
    block(
      'MASTER_PAD_VERSION_INFO',
      tag('MASTER_PAD_VERSION', '4.0'),
      tag('MASTER_PAD_EDITOR', 'Multron Community'),
      tag(
        'MASTER_PAD_INFO',
        'Portable Application Description, or PAD for short, is a data set that is used by shareware authors to disseminate information to anyone interested in their software products. To find out more go to http://pad.asp-software.org'
      )
    ),
    block(
      'Company_Info',
      tag('Company_Name', 'Multron Community'),
      tag('Address_1'),
      tag('Address_2'),
      tag('City_Town'),
      tag('State_Province'),
      tag('Zip_Postal_Code'),
      tag('Country'),
      tag('Company_WebSite_URL', REPO),
      block(
        'Contact_Info',
        tag('Author_First_Name', 'Multron'),
        tag('Author_Last_Name', 'Community'),
        tag('Author_Email', EMAIL),
        tag('Contact_First_Name', 'Multron'),
        tag('Contact_Last_Name', 'Community'),
        tag('Contact_Email', EMAIL)
      ),
      block(
        'Support_Info',
        tag('Sales_Email', EMAIL),
        tag('Support_Email', EMAIL),
        tag('General_Email', EMAIL),
        tag('Sales_Phone'),
        tag('Support_Phone'),
        tag('General_Phone'),
        tag('Fax_Phone')
      )
    ),
    block(
      'Program_Info',
      tag('Program_Name', 'Multron Download Manager'),
      tag('Program_Version', version),
      tag('Program_Release_Month', month),
      tag('Program_Release_Day', day),
      tag('Program_Release_Year', year),
      tag('Program_Cost_Dollars', '0'),
      tag('Program_Cost_Other_Code', 'USD'),
      tag('Program_Cost_Other', '0'),
      tag('Program_Type', 'Freeware'),
      tag('Program_Release_Status', 'Minor Update'),
      tag('Program_Install_Support', 'Install and Uninstall'),
      tag('Program_OS_Support', 'Windows 10,Windows 11,Mac OS X,Linux'),
      tag(
        'Program_Language',
        'English,Turkish,German,French,Spanish,Italian,Portuguese,Russian,Japanese,Korean,ChineseSimplified,ChineseTraditional'
      ),
      tag('Program_Change_Info', changeInfo(version)),
      tag('Program_Specific_Category', 'Download Managers'),
      tag('Program_Category_Class', 'Internet::Download Managers'),
      tag(
        'Program_System_Requirements',
        'Windows 10 or 11 (x64), macOS 11 or later (Apple Silicon or Intel), or Linux with glibc 2.36 or newer (x86_64 or arm64). WebView2 on Windows is installed automatically.'
      ),
      block(
        'File_Info',
        tag('File_Size_Bytes', size),
        tag('File_Size_K', Math.round(size / 1024)),
        tag('File_Size_MB', (size / 1024 / 1024).toFixed(2))
      ),
      block(
        'Expire_Info',
        tag('Has_Expire_Info', 'N'),
        tag('Expire_Count'),
        tag('Expire_Based_On', 'Days'),
        tag('Expire_Other_Info'),
        tag('Expire_Month'),
        tag('Expire_Day'),
        tag('Expire_Year')
      )
    ),
    block(
      'Program_Descriptions',
      block('English', tag('Keywords', KEYWORDS), ...Object.entries(DESCRIPTIONS).map(([k, v]) => tag(k, v)))
    ),
    block(
      'Web_Info',
      block(
        'Application_URLs',
        tag('Application_Info_URL', REPO),
        tag('Application_Order_URL'),
        tag('Application_Screenshot_URL', `${RAW}/docs/screenshots/downloads-dark.png`),
        tag('Application_Icon_URL', `${RAW}/src-tauri/icons/32x32.png`),
        tag('Application_XML_File_URL', `${REPO}/releases/latest/download/${PAD_NAME}`)
      ),
      block(
        'Download_URLs',
        tag('Primary_Download_URL', `${REPO}/releases/download/v${version}/${setup}`),
        tag('Secondary_Download_URL', `${REPO}/releases/latest`),
        tag('Additional_Download_URL_1'),
        tag('Additional_Download_URL_2')
      )
    ),
    block(
      'Permissions',
      tag(
        'Distribution_Permissions',
        `You may share the official, unchanged installers of Multron Download Manager free of charge. Selling them, bundling them with other software or changing them is not allowed. The full terms are in the license: ${LICENSE_URL}`
      ),
      tag(
        'EULA',
        `Multron Download Manager is source-available, proprietary software. You may install and use it, and share the official installers unchanged and free of charge. The source code may be read but not copied, reused or modified. Full license: ${LICENSE_URL}`
      )
    )
  )}\n`;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const opt = (name) => {
    const i = args.indexOf(`--${name}`);
    return i === -1 ? undefined : args[i + 1];
  };
  const version = opt('version') ?? appVersion();
  const installer = opt('installer');
  if (!installer || !fs.existsSync(installer)) {
    console.error('Usage: node scripts/pad.mjs --installer <path to the Windows setup> [--version x.y.z] [--date YYYY-MM-DD] [--out file]');
    process.exit(2);
  }
  const date = opt('date') ?? new Date().toISOString().slice(0, 10);
  const out = opt('out') ?? path.join(root, 'dist', PAD_NAME);
  fs.mkdirSync(path.dirname(out), { recursive: true });
  fs.writeFileSync(out, padXml({ version, date, size: fs.statSync(installer).size }));
  console.log(`PAD file for ${version}: ${out}`);
}
