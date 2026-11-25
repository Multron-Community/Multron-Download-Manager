const DECOY = 'pdf|docx?|docm|xlsx?|xlsm|pptx?|odt|ods|odp|rtf|txt|csv|md|html?|xml|json|epub|jpe?g|png|gif|bmp|webp|heic|tiff?|svg|ico|mp3|wav|flac|aac|ogg|m4a|wma|mp4|mkv|avi|mov|wmv|webm|flv|m4v|zip|rar|7z|tar|gz|iso';
export const DISGUISED_NAME = new RegExp(
  `\\.(${DECOY})[\\s_]*\\.(exe|msi|bat|cmd|com|scr|pif|ps1|vbs|vbe|js|jse|wsf|hta|cpl|lnk|jar)$|[\\u202a-\\u202e\\u2066-\\u2069]`,
  'i'
);

export const PROGRAM_FILE = /\.(exe|msi|msix|msixbundle|appx|appxbundle|bat|cmd|com|scr|pif|ps1|vbs|vbe|js|jse|wsf|hta|cpl|lnk|reg|jar|appimage|run|sh|deb|rpm|pkg|dmg|command)$/i;
