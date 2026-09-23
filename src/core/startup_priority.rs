//! Desktop-only scheduling policy, applied before creating application workers.
use std::{ffi::c_void, io};

const ABOVE_NORMAL_PRIORITY_CLASS: u32 = 0x0000_8000;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
    fn SetPriorityClass(process: *mut c_void, priority: u32) -> i32;
    fn GetPriorityClass(process: *mut c_void) -> u32;
}

pub fn apply() -> io::Result<()> {
    // AboveNormal gives every application thread a higher base priority while
    // retaining its relative thread priority. Do not use High or Realtime.
    unsafe {
        let process = GetCurrentProcess();
        if SetPriorityClass(process, ABOVE_NORMAL_PRIORITY_CLASS) == 0 {
            return Err(io::Error::last_os_error());
        }
        let actual = GetPriorityClass(process);
        if actual == 0 {
            return Err(io::Error::last_os_error());
        }
        if actual != ABOVE_NORMAL_PRIORITY_CLASS {
            return Err(io::Error::other("进程优先级未达到高于正常"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn desktop_priority_is_applied_in_isolated_process() {
        const CHILD: &str = "HARMONICA_PRIORITY_TEST_CHILD";
        if std::env::var_os(CHILD).is_some() {
            super::apply().unwrap();
            assert_eq!(
                unsafe { super::GetPriorityClass(super::GetCurrentProcess()) },
                super::ABOVE_NORMAL_PRIORITY_CLASS
            );
            return;
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "startup_priority::tests::desktop_priority_is_applied_in_isolated_process",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success());
    }
}
