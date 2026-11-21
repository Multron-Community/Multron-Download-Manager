use std::path::PathBuf;

pub fn supported() -> bool {
    cfg!(windows)
}

#[cfg(windows)]
pub fn start(window: &tauri::WebviewWindow, files: Vec<PathBuf>) -> anyhow::Result<()> {
    let hwnd = window.hwnd()?.0 as isize;
    window.run_on_main_thread(move || {
        let _ = drag(hwnd, &files);
    })?;
    Ok(())
}

#[cfg(not(windows))]
pub fn start(_window: &tauri::WebviewWindow, _files: Vec<PathBuf>) -> anyhow::Result<()> {
    anyhow::bail!("dragging files out is not supported on this system")
}

#[cfg(windows)]
fn drag(hwnd: isize, files: &[PathBuf]) -> windows::core::Result<()> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::IDataObject;
    use windows::Win32::System::Ole::{IDropSource, DROPEFFECT_COPY, DROPEFFECT_LINK};
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{ILCreateFromPathW, ILFree, SHCreateDataObject, SHDoDragDrop};

    unsafe {
        let pidls: Vec<*mut ITEMIDLIST> = files
            .iter()
            .map(|f| ILCreateFromPathW(&HSTRING::from(f.as_os_str())))
            .filter(|p| !p.is_null())
            .collect();
        if pidls.is_empty() {
            return Ok(());
        }
        let items: Vec<*const ITEMIDLIST> = pidls.iter().map(|p| p.cast_const()).collect();
        let result = SHCreateDataObject::<_, IDataObject>(None, Some(&items), None::<&IDataObject>)
            .and_then(|data| {
                SHDoDragDrop(
                    Some(HWND(hwnd as *mut core::ffi::c_void)),
                    &data,
                    None::<&IDropSource>,
                    DROPEFFECT_COPY | DROPEFFECT_LINK,
                )
            })
            .map(|_| ());
        for p in pidls {
            ILFree(Some(p.cast_const()));
        }
        result
    }
}
