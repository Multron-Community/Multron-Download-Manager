use std::path::PathBuf;

pub fn supported() -> bool {
    cfg!(windows)
}

#[cfg(windows)]
pub fn start(window: &tauri::WebviewWindow, files: Vec<PathBuf>) -> anyhow::Result<()> {
    let hwnd = window.hwnd()?.0 as isize;
    window.run_on_main_thread(move || {
        if let Err(e) = drag(hwnd, &files) {
            eprintln!("drag out failed: {e}");
        }
    })?;
    Ok(())
}

#[cfg(not(windows))]
pub fn start(_window: &tauri::WebviewWindow, _files: Vec<PathBuf>) -> anyhow::Result<()> {
    anyhow::bail!("dragging files out is not supported on this system")
}

#[cfg(windows)]
pub fn file_list(files: &[PathBuf]) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    let mut names: Vec<u16> = Vec::new();
    for f in files {
        names.extend(f.as_os_str().encode_wide());
        names.push(0);
    }
    names.push(0);
    let header: u32 = 20;
    let mut out = Vec::with_capacity(header as usize + names.len() * 2);
    out.extend_from_slice(&header.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&1i32.to_le_bytes());
    for unit in names {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

#[cfg(windows)]
#[windows::core::implement(windows::Win32::System::Ole::IDropSource)]
struct DropSource;

#[cfg(windows)]
impl windows::Win32::System::Ole::IDropSource_Impl for DropSource_Impl {
    fn QueryContinueDrag(
        &self,
        escape: windows::core::BOOL,
        _keys: windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS,
    ) -> windows::core::HRESULT {
        use windows::Win32::Foundation::{DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, S_OK};
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON,
        };
        use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SWAPBUTTON};
        if escape.as_bool() {
            return DRAGDROP_S_CANCEL;
        }
        let button = if unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0 {
            VK_RBUTTON
        } else {
            VK_LBUTTON
        };
        if unsafe { GetAsyncKeyState(i32::from(button.0)) } as u16 & 0x8000 == 0 {
            return DRAGDROP_S_DROP;
        }
        S_OK
    }

    fn GiveFeedback(
        &self,
        _effect: windows::Win32::System::Ole::DROPEFFECT,
    ) -> windows::core::HRESULT {
        windows::Win32::Foundation::DRAGDROP_S_USEDEFAULTCURSORS
    }
}

#[cfg(windows)]
fn drag(hwnd: isize, files: &[PathBuf]) -> windows::core::Result<u32> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        IDataObject, DVASPECT_CONTENT, FORMATETC, STGMEDIUM, TYMED_HGLOBAL,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::{IDropSource, DROPEFFECT_COPY, DROPEFFECT_LINK};
    use windows::Win32::UI::Shell::{SHCreateDataObject, SHDoDragDrop};

    if files.is_empty() {
        return Ok(0);
    }
    let bytes = file_list(files);
    unsafe {
        let data = SHCreateDataObject::<_, IDataObject>(None, None, None::<&IDataObject>)?;
        let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len())?;
        let target = GlobalLock(memory) as *mut u8;
        if target.is_null() {
            return Err(windows::core::Error::from_thread());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
        let _ = GlobalUnlock(memory);
        let format = FORMATETC {
            cfFormat: 15,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };
        let mut medium = STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            ..Default::default()
        };
        medium.u.hGlobal = memory;
        data.SetData(&format, &medium, true)?;
        let source: IDropSource = DropSource.into();
        let effect = SHDoDragDrop(
            Some(HWND(hwnd as *mut core::ffi::c_void)),
            &data,
            &source,
            DROPEFFECT_COPY | DROPEFFECT_LINK,
        )?;
        Ok(effect.0)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn lists_files_for_the_drop_target() {
        let files = vec![PathBuf::from(r"C:\a\one.txt"), PathBuf::from(r"D:\b\ğ.mp4")];
        let bytes = file_list(&files);
        assert_eq!(&bytes[..4], &20u32.to_le_bytes());
        assert_eq!(&bytes[16..20], &1i32.to_le_bytes());
        let units: Vec<u16> = bytes[20..]
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let text = String::from_utf16(&units).unwrap();
        assert_eq!(text, "C:\\a\\one.txt\0D:\\b\\ğ.mp4\0\0");
    }
}
