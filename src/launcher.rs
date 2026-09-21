//! Built separately by scripts/build.ps1: this executable has no WinUI dependency.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    if std::env::args_os().len() > 1 {
        // A GUI-subsystem binary needs a console for direct CLI use, but must
        // leave redirected output handles intact for pipelines and packaging.
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetStdHandle(kind: u32) -> *mut std::ffi::c_void;
            fn GetFileType(handle: *mut std::ffi::c_void) -> u32;
            fn AttachConsole(process: u32) -> i32;
        }
        unsafe {
            if GetFileType(GetStdHandle(-11_i32 as u32)) == 0 {
                AttachConsole(u32::MAX);
            }
        }
    }
    let result = std::env::current_exe().and_then(|exe| {
        let root = exe
            .parent()
            .ok_or_else(|| std::io::Error::other("无法定位程序目录"))?;
        // Preserve the caller's working directory, OS arguments and redirected handles.
        std::process::Command::new(root.join("program/HarmonicaStudio.exe"))
            .args(std::env::args_os().skip(1))
            .status()
    });
    match result {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => {
            let message = format!(
                "无法启动口琴工坊：{error}\n请保留完整文件夹，包括 program 目录。重新打包可修复程序文件。"
            );
            eprintln!("{message}");
            #[cfg(windows)]
            if std::env::args_os().len() == 1 {
                show_error(&message);
            }
            std::process::exit(1);
        }
    }
}

#[cfg(windows)]
fn show_error(message: &str) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(
            window: *mut std::ffi::c_void,
            text: *const u16,
            title: *const u16,
            flags: u32,
        ) -> i32;
    }
    let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    let title: Vec<u16> = "口琴工坊".encode_utf16().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x10);
    }
}
