//! Real application workflows with isolated temporary data and silent backends.
use crate::{
    controller::AppController,
    midi,
    models::{Note, Options},
    paths, project, service,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

pub fn synthetic_project(count: u32) -> Result<project::Project> {
    ensure!(
        (1..=100_000).contains(&count),
        "音符规模必须为 1 至 100000。"
    );
    let step = (900.0 / f64::from(count)).min(0.08);
    let notes = (0..count)
        .map(|index| Note {
            pitch: 60 + (index % 12) as i32,
            start: f64::from(index) * step,
            end: (f64::from(index) + 0.65) * step,
            velocity: 80,
        })
        .collect();
    project::make_project(notes, "诊断合成曲谱")
}

pub fn self_test() -> Result<Value> {
    let workspace = tempfile::Builder::new()
        .prefix("harmonica-rust-smoke-")
        .tempdir()?;
    let root = workspace.path();
    let notes = vec![
        Note {
            pitch: 48,
            start: 0.0,
            end: 0.20,
            velocity: 80,
        },
        Note {
            pitch: 61,
            start: 0.21,
            end: 0.40,
            velocity: 80,
        },
        Note {
            pitch: 74,
            start: 5.0,
            end: 5.20,
            velocity: 80,
        },
        Note {
            pitch: 85,
            start: 5.22,
            end: 5.45,
            velocity: 80,
        },
    ];
    let mut original = project::make_project(notes, "中文路径_验收")?;
    original.highlight = Some(5.0);
    original.options = Some(serde_json::to_value(Options {
        skip_long_rests: true,
        ..Options::default()
    })?);
    let project_path = root.join("工程.hstudio");
    project::save_project(&project_path, &original)?;
    ensure!(
        project::load_project(&project_path)? == original,
        "工程往返结果不一致。"
    );
    // Replace an existing project as well as creating a new one.
    project::save_project(&project_path, &original)?;
    let result =
        service::export_project(&original, &root.join("exports"), &AtomicBool::new(false))?;
    for name in [
        "工程.hstudio",
        "演奏脚本.ahk",
        "口琴单旋律.mid",
        "试听.wav",
        "音符.json",
        "按键时间表.json",
        "转换报告.json",
    ] {
        ensure!(result.folder.join(name).is_file(), "导出缺少 {name}");
    }
    let exported = project::load_project(&result.folder.join("工程.hstudio"))?;
    ensure!(
        original.notes == exported.notes && original.highlight == exported.highlight,
        "导出改变了原谱时间或标记。"
    );
    let (parts, _) = midi::read_midi(&result.folder.join("口琴单旋律.mid"))?;
    let output_pitches: Vec<i32> = parts
        .values()
        .flat_map(|notes| notes.iter().map(|n| n.pitch))
        .collect();
    let expected: Vec<i32> = original.notes.iter().map(|n| n.pitch).collect();
    ensure!(output_pitches == expected, "MIDI 音高不一致。");
    let actual: Vec<Note> = serde_json::from_slice(&fs::read(result.folder.join("音符.json"))?)?;
    ensure!(
        actual.iter().map(|n| n.pitch).collect::<Vec<_>>() == expected,
        "按键回读音高不一致。"
    );
    let wav = fs::read(result.folder.join("试听.wav"))?;
    ensure!(
        wav.starts_with(b"RIFF") && wav.len() > 44,
        "试听 WAV 格式无效。"
    );
    let ahk = paths::application_root().join("third_party/AutoHotkey/AutoHotkey64.exe");
    let mut ahk_validated = false;
    #[cfg(windows)]
    if ahk.is_file() {
        use std::os::windows::process::CommandExt;
        let output = std::process::Command::new(&ahk)
            .arg(result.folder.join("演奏脚本.ahk"))
            .arg("--validate")
            .creation_flags(0x08000000)
            .output()?;
        ensure!(
            output.status.success(),
            "AHK 无按键校验失败：{}",
            String::from_utf8_lossy(&output.stdout)
        );
        ahk_validated = true;
    }
    #[cfg(not(windows))]
    let _ = ahk;
    let mut controller = AppController::silent(root.join("controller"))?;
    controller.set_project(original.clone())?;
    controller.save_project(&root.join("控制层保存.hstudio"))?;
    controller.wait_idle(Duration::from_secs(15))?;
    ensure!(
        project::load_project(&root.join("控制层保存.hstudio"))?.notes == original.notes,
        "控制层保存不一致。"
    );
    controller.close()?;
    controller.wait_idle(Duration::from_secs(15))?;
    Ok(
        json!({"ok":true,"version":env!("CARGO_PKG_VERSION"),"backend":"Rust / WinUI 3","python_required":false,"desktop_enabled":cfg!(all(windows,feature="desktop")),"project_roundtrip":true,"export_consistency":true,"controller_save":true,"ahk_no_input_validation":ahk_validated,"notes":original.notes.len()}),
    )
}

pub fn diagnose(notes: u32, repeat: u32, scenario: &str, timeout_seconds: u64) -> Result<Value> {
    ensure!((1..=100).contains(&repeat), "重复次数必须为 1 至 100。");
    ensure!(
        ["save", "export", "edit", "load", "library"].contains(&scenario),
        "未知诊断场景。"
    );
    let workspace = tempfile::Builder::new()
        .prefix("harmonica-rust-diagnose-")
        .tempdir()?;
    let original = synthetic_project(notes)?;
    let source = workspace.path().join("diagnostic.mid");
    midi::write_midi(&original.notes, &source)?;
    let library = workspace.path().join("synthetic-library");
    let library_files = if scenario == "library" {
        fs::create_dir_all(&library)?;
        for index in 0..8 {
            midi::write_midi(
                &original.notes,
                &library.join(format!("synthetic-{index}.mid")),
            )?;
        }
        8
    } else {
        0
    };
    let mut samples = Vec::new();
    for run in 0..repeat {
        let mut controller = AppController::silent(workspace.path().join(format!("run-{run}")))?;
        controller.set_project(original.clone())?;
        if scenario == "library" {
            controller.preferences.library_folder = library.to_string_lossy().into_owned();
        }
        let start = Instant::now();
        match scenario {
            "save" => {
                controller.save_project(&workspace.path().join(format!("save-{run}.hstudio")))?
            }
            "export" => controller.export()?,
            "edit" => {
                let mut edited = original.notes.clone();
                for note in &mut edited {
                    note.pitch += 1;
                }
                controller.set_notes(edited)?;
            }
            "load" => controller.open_path(&source)?,
            "library" => controller.refresh_library()?,
            _ => unreachable!(),
        }
        let dispatch_ms = start.elapsed().as_secs_f64() * 1000.0;
        controller
            .wait_idle(Duration::from_secs(timeout_seconds))
            .context("诊断未在限时内成功完成")?;
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        if scenario == "library" {
            ensure!(
                controller.library.len() == library_files,
                "临时曲库扫描结果不完整。"
            );
        }
        if scenario == "save" {
            ensure!(
                project::load_project(&workspace.path().join(format!("save-{run}.hstudio")))?.notes
                    == original.notes,
                "保存校验失败。"
            );
        }
        samples.push(json!({"run":run+1,"dispatch_ms":dispatch_ms,"completed_ms":elapsed_ms}));
        controller.close()?;
        controller.wait_idle(Duration::from_secs(timeout_seconds))?;
    }
    let mean = samples
        .iter()
        .map(|s| s["completed_ms"].as_f64().unwrap())
        .sum::<f64>()
        / f64::from(repeat);
    Ok(
        json!({"schema_version":1,"ok":true,"version":env!("CARGO_PKG_VERSION"),"scenario":scenario,"notes":notes,"library_files":library_files,"repeat":repeat,"display_backend":"none","audio_backend":"silent","game_input":false,"samples":samples,"mean_completed_ms":mean,"measurement":"控制层提交至后台完成；包含轮询等待，不测量界面延迟"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_score_respects_project_limits() {
        let score = synthetic_project(100_000).unwrap();
        assert_eq!(score.notes.len(), 100_000);
        assert!(score.notes.last().unwrap().end <= 1200.0);
        assert!(synthetic_project(0).is_err());
    }
    #[test]
    fn library_diagnostics_scan_only_the_synthetic_library() {
        let report = diagnose(8, 1, "library", 10).unwrap();
        assert_eq!(report["ok"], true);
        assert_eq!(report["library_files"], 8);
    }
    #[test]
    fn large_synthetic_score_keeps_exact_timing_after_save() {
        let original = synthetic_project(10_000).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("large.hstudio");
        project::save_project(&file, &original).unwrap();
        let loaded = project::load_project(&file).unwrap();
        assert_eq!(original.notes.len(), loaded.notes.len());
        for (index, (before, after)) in original.notes.iter().zip(&loaded.notes).enumerate() {
            assert_eq!(
                before,
                after,
                "note {index}, start bits {:x}/{:x}, end bits {:x}/{:x}",
                before.start.to_bits(),
                after.start.to_bits(),
                before.end.to_bits(),
                after.end.to_bits()
            );
        }
    }
    #[test]
    fn save_diagnostics_use_real_isolated_controller() {
        let report = diagnose(12, 2, "save", 10).unwrap();
        assert_eq!(report["ok"], true);
        assert_eq!(report["samples"].as_array().unwrap().len(), 2);
    }
}
