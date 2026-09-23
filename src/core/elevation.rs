//! Elevate desktop startup only; CLI commands retain their redirected handles.
use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

#[link(name = "shell32")]
unsafe extern "system" {
    fn IsUserAnAdmin() -> i32;
    fn ShellExecuteW(
        window: isize,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show: i32,
    ) -> isize;
}

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

// Windows argv quoting: double backslashes before quotes and the closing quote.
fn parameters(args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Vec<u16> {
    let mut result = Vec::new();
    for arg in args {
        if !result.is_empty() {
            result.push(32);
        }
        result.push(34);
        let mut slashes = 0;
        for unit in arg.as_ref().encode_wide() {
            if unit == 92 {
                slashes += 1;
                continue;
            }
            result.extend(std::iter::repeat_n(
                92,
                if unit == 34 { slashes * 2 + 1 } else { slashes },
            ));
            result.push(unit);
            slashes = 0;
        }
        result.extend(std::iter::repeat_n(92, slashes * 2));
        result.push(34);
    }
    result.push(0);
    result
}

/// Returns true only after Windows accepts the elevated replacement process.
pub fn relaunch_if_needed() -> anyhow::Result<bool> {
    if unsafe { IsUserAnAdmin() } != 0 {
        return Ok(false);
    }
    let executable = wide(std::env::current_exe()?.as_os_str());
    let directory = wide(std::env::current_dir()?.as_os_str());
    let operation = wide(OsStr::new("runas"));
    let args = parameters(std::env::args_os().skip(1));
    let result = unsafe {
        ShellExecuteW(
            0,
            operation.as_ptr(),
            executable.as_ptr(),
            args.as_ptr(),
            directory.as_ptr(),
            1,
        )
    };
    anyhow::ensure!(
        result > 32,
        "管理员启动未完成（系统返回 {result}）。请在 Windows 权限提示中允许启动；取消后程序不会以普通权限继续运行。"
    );
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn elevated_arguments_roundtrip_through_windows_parser() {
        #[link(name = "shell32")]
        unsafe extern "system" {
            fn CommandLineToArgvW(line: *const u16, count: *mut i32) -> *mut *mut u16;
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn LocalFree(memory: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        }
        let args = [
            "studio.exe",
            "gui",
            "--file",
            "D:\\中文 曲谱\\末尾\\",
            "",
            "a\\\"b",
            "--remote",
        ];
        let line = parameters(args);
        let mut count = 0;
        unsafe {
            let parsed = CommandLineToArgvW(line.as_ptr(), &mut count);
            assert!(!parsed.is_null());
            assert_eq!(count as usize, args.len());
            for (index, expected) in args.iter().enumerate() {
                let ptr = *parsed.add(index);
                let mut len = 0;
                while *ptr.add(len) != 0 {
                    len += 1;
                }
                assert_eq!(
                    std::slice::from_raw_parts(ptr, len),
                    expected.encode_utf16().collect::<Vec<_>>()
                );
            }
            LocalFree(parsed.cast());
        }
    }
}
