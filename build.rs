use std::{env, fs, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=assets/studio.ico");
    if env::var_os("CARGO_FEATURE_DESKTOP").is_none()
        || env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
    {
        return;
    }
    windows_reactor_setup::as_self_contained();
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let profile = env::var("PROFILE").unwrap();
    let runtime = out
        .ancestors()
        .find(|p| p.file_name().is_some_and(|s| s == profile.as_str()))
        .expect("Cargo profile output");
    for required in [
        "Microsoft.UI.Xaml.dll",
        "Microsoft.WindowsAppRuntime.dll",
        "resources.pri",
        "Microsoft.Web.WebView2.Core.dll",
    ] {
        let path = runtime.join(required);
        assert!(
            fs::metadata(&path).is_ok_and(|m| m.len() > 0),
            "WinUI runtime staging failed: {}. Verify NuGet access and remove the failed package cache before retrying.",
            path.display()
        );
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let rc = out.join("studio.rc");
    let icon = root
        .join("assets/studio.ico")
        .to_string_lossy()
        .replace('\\', "/");
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let numeric_version = ["MAJOR", "MINOR", "PATCH"]
        .map(|part| env::var(format!("CARGO_PKG_VERSION_{part}")).unwrap())
        .join(",");
    let resource = format!(
        "1 ICON \"{icon}\"\n1 VERSIONINFO\nFILEVERSION {numeric_version},0\nPRODUCTVERSION {numeric_version},0\nFILEOS 0x40004L\nFILETYPE 1L\nBEGIN\n BLOCK \"StringFileInfo\"\n BEGIN\n  BLOCK \"040904B0\"\n  BEGIN\n   VALUE \"FileDescription\", \"Harmonica Studio\"\n   VALUE \"FileVersion\", \"{version}\"\n   VALUE \"ProductName\", \"Harmonica Studio\"\n   VALUE \"ProductVersion\", \"{version}\"\n  END\n END\n BLOCK \"VarFileInfo\"\n BEGIN\n  VALUE \"Translation\", 0x0409, 1200\n END\nEND\n"
    );
    fs::write(&rc, resource).unwrap();
    let mut candidates = vec![PathBuf::from("rc.exe")];
    if let Some(programs) = env::var_os("ProgramFiles(x86)") {
        let sdk = PathBuf::from(programs).join("Windows Kits/10/bin");
        if let Ok(entries) = fs::read_dir(sdk) {
            let mut dirs: Vec<_> = entries.flatten().map(|e| e.path()).collect();
            dirs.sort();
            for dir in dirs.into_iter().rev() {
                let candidate = dir.join("x64/rc.exe");
                if candidate.is_file() {
                    candidates.insert(0, candidate);
                }
            }
        }
    }
    let res = out.join("studio.res");
    let mut compiled = false;
    for compiler in candidates {
        if Command::new(&compiler)
            .arg("/nologo")
            .arg("/c65001")
            .arg("/fo")
            .arg(&res)
            .arg(&rc)
            .status()
            .is_ok_and(|s| s.success())
        {
            compiled = true;
            break;
        }
    }
    assert!(
        compiled,
        "Windows SDK resource compiler rc.exe is required for the application icon."
    );
    println!("cargo:rustc-link-arg-bins={}", res.display());
    // The standalone launcher reuses only icon/version resources, never the WinUI manifest.
    fs::copy(&res, runtime.join("studio-launcher.res")).unwrap();
}
