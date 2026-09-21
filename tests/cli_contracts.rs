#![cfg(feature = "desktop")]
use harmonica_studio::{
    midi,
    models::{Note, Options},
    project,
};
use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
};
fn run(args: &[&str], cwd: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_HarmonicaStudio"));
    command.args(args).current_dir(cwd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.output().unwrap()
}
fn score() -> project::Project {
    project::make_project(
        vec![
            Note {
                pitch: 60,
                start: 0.0,
                end: 0.1,
                velocity: 80,
            },
            Note {
                pitch: 64,
                start: 0.12,
                end: 0.22,
                velocity: 90,
            },
        ],
        "命令行验收",
    )
    .unwrap()
}
#[test]
fn inspect_keeps_raw_midi_full_range_and_emits_json() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("原始.mid");
    midi::write_midi(
        &[
            Note {
                pitch: 30,
                start: 0.,
                end: 0.1,
                velocity: 80,
            },
            Note {
                pitch: 100,
                start: 0.2,
                end: 0.3,
                velocity: 80,
            },
        ],
        &path,
    )
    .unwrap();
    let result = run(&["inspect", path.to_str().unwrap()], dir.path());
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let json: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json[0]["notes"], 2);
    assert_eq!(json[0]["channel"], 1);
}
#[test]
fn export_does_not_apply_historical_transpose_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("工程.hstudio");
    let mut original = score();
    original.options = Some(
        serde_json::to_value(Options {
            transpose: 12,
            ..Options::default()
        })
        .unwrap(),
    );
    original.highlight = Some(0.12);
    project::save_project(&path, &original).unwrap();
    let output = dir.path().join("输出");
    let result = run(
        &[
            "export",
            path.to_str().unwrap(),
            "--out",
            output.to_str().unwrap(),
        ],
        dir.path(),
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let json: Value = serde_json::from_slice(&result.stdout).unwrap();
    let folder = Path::new(json["folder"].as_str().unwrap());
    let after = project::load_project(&folder.join("工程.hstudio")).unwrap();
    assert_eq!(after.notes, original.notes);
    assert_eq!(after.highlight, original.highlight);
}
#[test]
fn bad_channel_rejected_before_creating_output() {
    let dir = tempfile::tempdir().unwrap();
    let result = run(&["convert", "missing.mid", "--channel", "0"], dir.path());
    assert!(!result.status.success());
    assert!(!dir.path().join("data").exists());
}
#[test]
fn diagnostics_emit_valid_json_and_do_not_use_user_home() {
    let dir = tempfile::tempdir().unwrap();
    let forbidden = dir.path().join("must-stay-absent");
    let mut command = Command::new(env!("CARGO_BIN_EXE_HarmonicaStudio"));
    command
        .args([
            "diagnose",
            "--scenario",
            "save",
            "--notes",
            "8",
            "--repeat",
            "1",
        ])
        .env("HARMONICA_STUDIO_HOME", &forbidden)
        .current_dir(dir.path());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let json: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["notes"], 8);
    assert!(!forbidden.exists());
}

#[test]
fn desktop_executable_reports_compiled_feature() {
    let dir = tempfile::tempdir().unwrap();
    let result = run(&["build-info"], dir.path());
    assert!(result.status.success());
    let info: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(info["desktop_enabled"], cfg!(windows));
}
#[test]
fn default_export_directory_honors_portable_data_override() {
    let dir = tempfile::tempdir().unwrap();
    let original = score();
    let file = dir.path().join("工程.hstudio");
    project::save_project(&file, &original).unwrap();
    let home = dir.path().join("configured-home");
    let mut command = Command::new(env!("CARGO_BIN_EXE_HarmonicaStudio"));
    command
        .args(["export", file.to_str().unwrap()])
        .env("HARMONICA_STUDIO_HOME", &home)
        .current_dir(dir.path());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let json: Value = serde_json::from_slice(&result.stdout).unwrap();
    let folder = Path::new(json["folder"].as_str().unwrap());
    // Windows canonicalization adds a verbatim prefix; compare resolved locations.
    assert!(
        folder
            .canonicalize()
            .unwrap()
            .starts_with(home.join("exports").canonicalize().unwrap()),
        "export directory: {}",
        folder.display()
    );
    assert!(!dir.path().join("data").exists());
    assert_eq!(
        project::load_project(&folder.join("工程.hstudio"))
            .unwrap()
            .notes,
        original.notes
    );
}
