//! Native Win32 COM STA folder picker dialog (`IFileOpenDialog`).

#[cfg(windows)]
pub async fn pick_folder(initial_dir: Option<String>) -> Result<Option<String>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let res = pick_folder_sta(initial_dir.as_deref());
        let _ = tx.send(res);
    });
    rx.await
        .map_err(|_| "Folder picker thread terminated unexpectedly".to_string())?
}

#[cfg(windows)]
fn pick_folder_sta(initial_dir: Option<&str>) -> Result<Option<String>, String> {
    use windows::{
        core::{w, HSTRING},
        Win32::{
            Foundation::{ERROR_CANCELLED, HWND},
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
                COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
            },
            UI::{
                Shell::{
                    FileOpenDialog, IFileOpenDialog, IShellItem, SHCreateItemFromParsingName,
                    FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
                },
                WindowsAndMessaging::{
                    CreateWindowExW, DestroyWindow, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
                },
            },
        },
    };

    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }

    struct OwnerGuard(HWND);
    impl Drop for OwnerGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }

    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)
            .ok()
            .map_err(|e| format!("COM initialization failed: {e}"))?;
        let _com = ComGuard;

        let owner = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!(""),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            None,
            None,
        )
        .map_err(|e| format!("Failed creating picker owner window: {e}"))?;
        let _owner_guard = OwnerGuard(owner);

        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL)
            .map_err(|e| format!("Failed creating IFileOpenDialog: {e}"))?;

        let opts = dialog.GetOptions().unwrap_or_default();
        dialog
            .SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST)
            .map_err(|e| format!("SetOptions failed: {e}"))?;

        if let Some(dir) = initial_dir.map(str::trim).filter(|s| !s.is_empty()) {
            if std::path::Path::new(dir).is_dir() {
                let wide = HSTRING::from(dir);
                if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&wide, None) {
                    let _ = dialog.SetFolder(&item);
                }
            }
        }

        match dialog.Show(Some(owner)) {
            Ok(()) => {}
            Err(err) if err.code() == ERROR_CANCELLED.to_hresult() => return Ok(None),
            Err(err) => return Err(format!("Folder dialog failed: {err}")),
        }

        let item = dialog
            .GetResult()
            .map_err(|e| format!("GetResult failed: {e}"))?;
        let pwstr = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|e| format!("GetDisplayName failed: {e}"))?;
        let path_str = pwstr
            .to_string()
            .map_err(|e| format!("Invalid UTF-16 path: {e}"));
        CoTaskMemFree(Some(pwstr.0 as _));
        path_str.map(Some)
    }
}

#[cfg(not(windows))]
pub async fn pick_folder(_initial_dir: Option<String>) -> Result<Option<String>, String> {
    Err("Native folder picker is only supported on Windows".to_string())
}
