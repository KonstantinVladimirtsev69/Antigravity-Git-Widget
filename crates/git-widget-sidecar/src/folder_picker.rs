//! Native Win32 COM STA folder picker dialog (`IFileOpenDialog`).

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
#[derive(Default)]
struct FocusRaiseOnce {
    done: AtomicBool,
}

#[cfg(windows)]
impl FocusRaiseOnce {
    fn try_claim(&self) -> bool {
        !self.done.swap(true, Ordering::SeqCst)
    }
}

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
        core::{implement, Interface, Ref, Result as WinResult, HSTRING},
        Win32::{
            Foundation::{ERROR_CANCELLED, HWND},
            System::{
                Com::{
                    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
                    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
                },
                Ole::IOleWindow,
                Threading::{AttachThreadInput, GetCurrentThreadId},
            },
            UI::{
                Shell::{
                    FileOpenDialog, IFileDialog, IFileDialogEvents, IFileDialogEvents_Impl,
                    IFileOpenDialog, IShellItem, SHCreateItemFromParsingName, FDEOR_DEFAULT,
                    FDESVR_DEFAULT, FDE_OVERWRITE_RESPONSE, FDE_SHAREVIOLATION_RESPONSE,
                    FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
                },
                WindowsAndMessaging::{
                    BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsWindow,
                    IsWindowVisible, SetForegroundWindow, SetWindowPos, HWND_NOTOPMOST,
                    HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE,
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

    #[implement(IFileDialogEvents)]
    struct PickerEvents {
        once: FocusRaiseOnce,
    }

    impl PickerEvents {
        fn raise_once(&self, pfd: &Ref<IFileDialog>) {
            let Some(dlg) = pfd.as_ref() else {
                return;
            };
            unsafe {
                let Ok(ole) = dlg.cast::<IOleWindow>() else {
                    return;
                };
                let Ok(dlg_hwnd) = ole.GetWindow() else {
                    return;
                };
                if dlg_hwnd.0.is_null() || !IsWindowVisible(dlg_hwnd).as_bool() {
                    return;
                }
                if !self.once.try_claim() {
                    return;
                }
                bring_to_foreground_once(dlg_hwnd);
            }
        }
    }

    impl IFileDialogEvents_Impl for PickerEvents_Impl {
        fn OnFileOk(&self, _pfd: Ref<IFileDialog>) -> WinResult<()> {
            Ok(())
        }
        fn OnFolderChanging(
            &self,
            _pfd: Ref<IFileDialog>,
            _psifolder: Ref<IShellItem>,
        ) -> WinResult<()> {
            Ok(())
        }
        fn OnFolderChange(&self, pfd: Ref<IFileDialog>) -> WinResult<()> {
            self.raise_once(&pfd);
            Ok(())
        }
        fn OnSelectionChange(&self, pfd: Ref<IFileDialog>) -> WinResult<()> {
            self.raise_once(&pfd);
            Ok(())
        }
        fn OnShareViolation(
            &self,
            _pfd: Ref<IFileDialog>,
            _psi: Ref<IShellItem>,
        ) -> WinResult<FDE_SHAREVIOLATION_RESPONSE> {
            Ok(FDESVR_DEFAULT)
        }
        fn OnTypeChange(&self, _pfd: Ref<IFileDialog>) -> WinResult<()> {
            Ok(())
        }
        fn OnOverwrite(
            &self,
            _pfd: Ref<IFileDialog>,
            _psi: Ref<IShellItem>,
        ) -> WinResult<FDE_OVERWRITE_RESPONSE> {
            Ok(FDEOR_DEFAULT)
        }
    }

    unsafe fn bring_to_foreground_once(hwnd: HWND) {
        let cur_fg = GetForegroundWindow();
        let cur_tid = GetCurrentThreadId();
        let fg_tid = if !cur_fg.0.is_null() {
            GetWindowThreadProcessId(cur_fg, None)
        } else {
            0
        };
        let attached = fg_tid != 0
            && fg_tid != cur_tid
            && AttachThreadInput(cur_tid, fg_tid, true).as_bool();
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE,
        );
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_NOTOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE,
        );
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ = AttachThreadInput(cur_tid, fg_tid, false);
        }
    }

    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)
            .ok()
            .map_err(|e| format!("COM initialization failed: {e}"))?;
        let _com = ComGuard;

        let host_fg = GetForegroundWindow();
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

        let events: IFileDialogEvents = PickerEvents {
            once: FocusRaiseOnce::default(),
        }
        .into();
        let cookie = dialog.Advise(&events).ok();
        let show_res = dialog.Show(None);
        if let Some(c) = cookie {
            let _ = dialog.Unadvise(c);
        }

        if !host_fg.0.is_null() && IsWindow(Some(host_fg)).as_bool() {
            let _ = SetForegroundWindow(host_fg);
        }

        match show_res {
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

#[cfg(all(test, windows))]
mod tests {
    use super::FocusRaiseOnce;

    #[test]
    fn focus_raise_once_triggers_strictly_once() {
        let guard = FocusRaiseOnce::default();
        assert!(guard.try_claim());
        assert!(!guard.try_claim());
        assert!(!guard.try_claim());
    }
}
