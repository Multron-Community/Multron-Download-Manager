use base64::Engine;

const ENTROPY: &[u8] = b"Multron-Download-Manager/secret/v1";
pub const PREFIX: &str = "dpapi:";

#[cfg(windows)]
fn blob(data: &[u8]) -> windows_sys::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB {
    windows_sys::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    }
}

#[cfg(windows)]
fn run(data: &[u8], encrypt: bool) -> Option<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };
    let input = blob(data);
    let entropy = blob(ENTROPY);
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                std::ptr::null(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 || output.pbData.is_null() {
        return None;
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(output.pbData as _);
    }
    Some(bytes)
}

#[cfg(windows)]
pub fn protect(plain: &str) -> Option<String> {
    let sealed = run(plain.as_bytes(), true)?;
    Some(format!(
        "{PREFIX}{}",
        base64::engine::general_purpose::STANDARD.encode(sealed)
    ))
}

#[cfg(not(windows))]
pub fn protect(_plain: &str) -> Option<String> {
    None
}

#[cfg(windows)]
pub fn unprotect(sealed: &str) -> Option<String> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(sealed.strip_prefix(PREFIX)?)
        .ok()?;
    String::from_utf8(run(&raw, false)?).ok()
}

#[cfg(not(windows))]
pub fn unprotect(_sealed: &str) -> Option<String> {
    None
}

pub fn restrict_file(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = path;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn seals_and_opens_a_secret_for_this_user() {
        let sealed = protect("my-api-key-123").unwrap();
        assert!(sealed.starts_with(PREFIX));
        assert!(!sealed.contains("my-api-key-123"));
        assert_eq!(unprotect(&sealed).as_deref(), Some("my-api-key-123"));
        assert!(unprotect("dpapi:bm90IHNlYWxlZA==").is_none());
        assert!(unprotect("plain").is_none());
    }
}
