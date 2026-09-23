use super::*;

pub(super) fn window_icon_path() -> &'static str {
    // Reactor retains a static path; resolve it once from the portable resource root.
    static PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PATH.get_or_init(|| crate::paths::icon_path().to_string_lossy().into_owned())
        .as_str()
}

/// Keep independent client sizes for the two modes, just as the source window
/// records its current size before changing the compact flag.
#[derive(Debug)]
pub(super) struct WindowSizes {
    full: (f64, f64),
    compact: (f64, f64),
}
impl Default for WindowSizes {
    fn default() -> Self {
        Self {
            full: (1220.0, 880.0),
            compact: (900.0, 650.0),
        }
    }
}
impl WindowSizes {
    pub(super) fn for_mode(&self, compact: bool) -> (f64, f64) {
        if compact { self.compact } else { self.full }
    }
    pub(super) fn remember_transition(
        &mut self,
        was_compact: bool,
        is_compact: bool,
        current: Option<(f64, f64)>,
    ) {
        if was_compact == is_compact {
            return;
        }
        if let Some((width, height)) =
            current.filter(|(w, h)| w.is_finite() && h.is_finite() && *w > 0.0 && *h > 0.0)
        {
            if was_compact {
                self.compact = (width, height);
            } else {
                self.full = (width, height);
            }
        }
    }
}
#[repr(C)]
#[derive(Default)]
struct ClientRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
#[link(name = "user32")]
unsafe extern "system" {
    fn GetClientRect(window: Hwnd, rect: *mut ClientRect) -> i32;
    fn GetDpiForWindow(window: Hwnd) -> u32;
    fn IsWindow(window: Hwnd) -> i32;
    fn IsIconic(window: Hwnd) -> i32;
    fn IsWindowVisible(window: Hwnd) -> i32;
}
pub(super) enum WindowVisibility {
    Visible,
    Minimized,
    Hidden,
    Unknown,
}
pub(super) fn window_visibility(window: Hwnd) -> WindowVisibility {
    if window.is_null() || unsafe { IsWindow(window) } == 0 {
        WindowVisibility::Unknown
    } else if unsafe { IsIconic(window) } != 0 {
        WindowVisibility::Minimized
    } else if unsafe { IsWindowVisible(window) } == 0 {
        WindowVisibility::Hidden
    } else {
        WindowVisibility::Visible
    }
}
pub(super) fn read_client_size(window: Hwnd) -> Option<(f64, f64)> {
    if window.is_null() {
        return None;
    }
    let mut rect = ClientRect::default();
    if unsafe { GetClientRect(window, &mut rect) } == 0 {
        return None;
    }
    let dpi = unsafe { GetDpiForWindow(window) };
    if dpi == 0 {
        return None;
    }
    let scale = 96.0 / f64::from(dpi);
    Some((
        f64::from(rect.right - rect.left) * scale,
        f64::from(rect.bottom - rect.top) * scale,
    ))
}
#[link(name = "user32")]
unsafe extern "system" {
    fn OpenClipboard(hwnd: Hwnd) -> i32;
    fn CloseClipboard() -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, memory: Hwnd) -> Hwnd;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalAlloc(flags: u32, bytes: usize) -> Hwnd;
    fn GlobalLock(memory: Hwnd) -> Hwnd;
    fn GlobalUnlock(memory: Hwnd) -> i32;
    fn GlobalFree(memory: Hwnd) -> Hwnd;
}
pub(super) fn copy_text(text: &str) -> Result<()> {
    let value: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let memory = GlobalAlloc(2, value.len() * 2);
        anyhow::ensure!(!memory.is_null(), "无法分配剪贴板空间");
        let target = GlobalLock(memory);
        if target.is_null() {
            GlobalFree(memory);
            anyhow::bail!("无法写入剪贴板");
        }
        std::ptr::copy_nonoverlapping(value.as_ptr(), target.cast::<u16>(), value.len());
        GlobalUnlock(memory);
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            GlobalFree(memory);
            anyhow::bail!("剪贴板正在使用中");
        }
        EmptyClipboard();
        let result = SetClipboardData(13, memory);
        CloseClipboard();
        if result.is_null() {
            GlobalFree(memory);
            anyhow::bail!("无法复制连接");
        }
    }
    Ok(())
}
// The public Reactor surface currently has no cancellable native closing callback.
// A UI-thread subclass only defers WM_CLOSE into the normal message queue. It never
// executes application work from a Windows callback or forces a process to exit.
struct CloseHook {
    sender: LocalSender<Message>,
    allowed: Rc<Cell<bool>>,
}
pub(super) type Hwnd = *mut std::ffi::c_void;
type SubclassProc = unsafe extern "system" fn(Hwnd, u32, usize, isize, usize, usize) -> isize;
#[link(name = "comctl32")]
unsafe extern "system" {
    fn SetWindowSubclass(hwnd: Hwnd, callback: Option<SubclassProc>, id: usize, data: usize)
    -> i32;
    fn RemoveWindowSubclass(hwnd: Hwnd, callback: Option<SubclassProc>, id: usize) -> i32;
    fn DefSubclassProc(hwnd: Hwnd, message: u32, wparam: usize, lparam: isize) -> isize;
}
unsafe extern "system" fn close_subclass(
    hwnd: Hwnd,
    message: u32,
    wparam: usize,
    lparam: isize,
    id: usize,
    data: usize,
) -> isize {
    const WM_CLOSE: u32 = 0x10;
    const WM_NCDESTROY: u32 = 0x82;
    if message == WM_CLOSE {
        let hook = unsafe { &*(data as *const CloseHook) };
        if !hook.allowed.get() {
            let _ = hook.sender.send(Message::Close);
            return 0;
        }
    }
    if message == WM_NCDESTROY {
        unsafe {
            RemoveWindowSubclass(hwnd, Some(close_subclass), id);
            drop(Box::from_raw(data as *mut CloseHook));
        }
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}
pub(super) fn install_close_hook(
    hwnd: Hwnd,
    sender: LocalSender<Message>,
    allowed: Rc<Cell<bool>>,
) -> std::result::Result<(), String> {
    let data = Box::into_raw(Box::new(CloseHook { sender, allowed }));
    if unsafe { SetWindowSubclass(hwnd, Some(close_subclass), 0x48535455, data as usize) } == 0 {
        unsafe {
            drop(Box::from_raw(data));
        }
        Err(format!(
            "无法安装保存保护：{}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}
/// Windows change notifications signal mutations without rescanning a library on each frame.
/// Refresh is debounced and submitted through the controller's existing background channel.
pub(super) struct LibraryWatch {
    root: PathBuf,
    handle: Option<*mut std::ffi::c_void>,
    last_change: Option<Instant>,
    last_retry: Instant,
}
impl Default for LibraryWatch {
    fn default() -> Self {
        Self {
            root: PathBuf::new(),
            handle: None,
            last_change: None,
            last_retry: Instant::now() - Duration::from_secs(3),
        }
    }
}
impl LibraryWatch {
    fn reset(&mut self) {
        if let Some(handle) = self.handle.take() {
            unsafe {
                FindCloseChangeNotification(handle);
            }
        }
    }
    pub(super) fn poll(&mut self, root: &std::path::Path) -> bool {
        use std::os::windows::ffi::OsStrExt;
        if self.root != root {
            self.reset();
            self.root = root.to_owned();
            self.last_retry = Instant::now() - Duration::from_secs(3);
            self.last_change = None;
        }
        if self.handle.is_none() && self.last_retry.elapsed() >= Duration::from_secs(2) {
            self.last_retry = Instant::now();
            let path: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
            let handle =
                unsafe { FindFirstChangeNotificationW(path.as_ptr(), 1, 0x01 | 0x02 | 0x10) };
            if !handle.is_null() && handle as isize != -1 {
                self.handle = Some(handle);
                self.last_change = Some(Instant::now());
            }
        }
        if let Some(handle) = self.handle {
            let result = unsafe { WaitForSingleObject(handle, 0) };
            if result == 0 {
                self.last_change = Some(Instant::now());
                if unsafe { FindNextChangeNotification(handle) } == 0 {
                    self.reset();
                }
            } else if result == u32::MAX {
                self.reset();
            }
        }
        if self
            .last_change
            .is_some_and(|at| at.elapsed() >= Duration::from_millis(400))
        {
            self.last_change = None;
            true
        } else {
            false
        }
    }
}
impl Drop for LibraryWatch {
    fn drop(&mut self) {
        self.reset();
    }
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn FindFirstChangeNotificationW(
        path: *const u16,
        subtree: i32,
        filter: u32,
    ) -> *mut std::ffi::c_void;
    fn FindNextChangeNotification(handle: *mut std::ffi::c_void) -> i32;
    fn FindCloseChangeNotification(handle: *mut std::ffi::c_void) -> i32;
    fn WaitForSingleObject(handle: *mut std::ffi::c_void, milliseconds: u32) -> u32;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    pub(super) fn native_library_watch_debounces_file_changes_without_directory_scan() {
        let directory = tempfile::tempdir().unwrap();
        let mut watcher = LibraryWatch::default();
        assert!(!watcher.poll(directory.path()));
        assert!(
            watcher.handle.is_some(),
            "Windows change notification handle must be available"
        );
        watcher.last_change = None;
        std::fs::write(
            directory.path().join("song.mid"),
            b"test change notification",
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if watcher.poll(directory.path()) {
                break;
            }
            assert!(Instant::now() < deadline, "file mutation was not published");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!watcher.poll(directory.path()));
    }
}
