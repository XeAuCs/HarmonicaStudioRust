//! Transactional conversion and export. Original score times are never rewritten.
use crate::{
    melody::{prepare, rank_parts},
    midi::{PartKey, Parts, TrackNames, read_midi, write_midi},
    models::{Note, Options},
    preview::{decode_events, render_wav},
    project::{Project, load_project, make_project, save_project, unique_id, validate_project},
    rests::compress_long_rests,
    schedule::build_events,
    transport::{TimeMap, playback_anchors},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
#[derive(Clone, Debug)]
pub struct LoadedParts {
    pub parts: Parts,
    pub names: TrackNames,
    pub keys: Vec<PartKey>,
    pub recommendations: std::collections::BTreeMap<PartKey, f64>,
}
#[derive(Clone, Debug)]
pub struct ExportResult {
    pub folder: PathBuf,
    pub report: Value,
    pub project: Project,
    pub actual: Vec<Note>,
    pub anchors: Vec<(f64, f64)>,
    pub to_audio: TimeMap,
    pub to_score: TimeMap,
}
pub fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Acquire), "转换已取消。");
    Ok(())
}
pub fn load_ranked_midi(source: &Path, cancel: &AtomicBool) -> Result<LoadedParts> {
    check_cancel(cancel)?;
    let (parts, names) = read_midi(source)?;
    check_cancel(cancel)?;
    let ranked = rank_parts(&parts, &names);
    let keys = ranked.iter().map(|(k, _)| *k).collect();
    let recommendations = ranked.into_iter().collect();
    check_cancel(cancel)?;
    Ok(LoadedParts {
        parts,
        names,
        keys,
        recommendations,
    })
}
fn prepare_export(
    folder: PathBuf,
    report: Value,
    project: Project,
    actual: Vec<Note>,
) -> ExportResult {
    let anchors = playback_anchors(&project.notes, &actual);
    let to_audio = TimeMap::new(anchors.iter().copied());
    let to_score = TimeMap::new(anchors.iter().map(|&(a, b)| (b, a)));
    ExportResult {
        folder,
        report,
        project,
        actual,
        anchors,
        to_audio,
        to_score,
    }
}
pub fn load_export_result(folder: &Path) -> Result<ExportResult> {
    let project = load_project(&folder.join("工程.hstudio"))?;
    let actual: Vec<Note> = serde_json::from_slice(&fs::read(folder.join("音符.json"))?)?;
    let report = serde_json::from_slice(&fs::read(folder.join("转换报告.json"))?)?;
    Ok(prepare_export(
        folder.to_path_buf(),
        report,
        project,
        actual,
    ))
}
pub fn convert(
    source: &Path,
    output_root: &Path,
    options: &Options,
    cancel: &AtomicBool,
) -> Result<ExportResult> {
    check_cancel(cancel)?;
    options.validate()?;
    let source = source.canonicalize().context("无法读取来源 MIDI。");
    let source = source?;
    let (parts, names) = read_midi(&source)?;
    check_cancel(cancel)?;
    let (notes, mut report) = prepare(&parts, &names, options)?;
    check_cancel(cancel)?;
    let source_hash = format!("{:x}", Sha256::digest(fs::read(&source)?));
    let source_path = display_path(&source);
    report["source"] = json!(source_path);
    report["source_sha256"] = json!(source_hash);
    report["options"] = serde_json::to_value(options)?;
    let mut project = make_project(
        notes,
        source.file_stem().unwrap_or_default().to_string_lossy(),
    )?;
    project.source = Some(json!({"path":source_path,"sha256":source_hash}));
    project.options = Some(serde_json::to_value(options)?);
    project.report = Some(report);
    export(&project, output_root, cancel, "midi_conversion")
}
fn display_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    if let Some(tail) = path.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{tail}")
    } else {
        path.strip_prefix("\\\\?\\").unwrap_or(&path).to_owned()
    }
}
pub fn export_project(
    project: &Project,
    output_root: &Path,
    cancel: &AtomicBool,
) -> Result<ExportResult> {
    export(project, output_root, cancel, "edited_project")
}
const OUTPUTS: [&str; 7] = [
    "演奏脚本.ahk",
    "口琴单旋律.mid",
    "音符.json",
    "按键时间表.json",
    "转换报告.json",
    "工程.hstudio",
    "试听.wav",
];
fn safe_label(title: &str) -> String {
    let mut label: String = title
        .chars()
        .map(|c| {
            if c < ' ' || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim_matches([' ', '.'])
        .chars()
        .take(60)
        .collect();
    if label.is_empty() {
        label = "曲谱".into();
    }
    let upper = label.split('.').next().unwrap_or("").to_uppercase();
    if ["CON", "PRN", "AUX", "NUL"].contains(&upper.as_str())
        || (0..=9).any(|i| upper == format!("COM{i}") || upper == format!("LPT{i}"))
    {
        label = format!("曲谱_{label}");
    }
    label
}
fn cleanup_staging(stage: &Path, parent: &Path) {
    // Delete only known files in this invocation's freshly-created stage, never recurse.
    if stage.parent() != Some(parent)
        || !stage
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with(".partial-"))
    {
        return;
    }
    let Ok(meta) = fs::symlink_metadata(stage) else {
        return;
    };
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return;
        }
    }
    for name in OUTPUTS {
        let path = stage.join(name);
        if let Ok(meta) = fs::symlink_metadata(&path) {
            if meta.file_type().is_symlink() || !meta.is_file() {
                return;
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return;
                }
            }
        }
    }
    for name in OUTPUTS {
        let _ = fs::remove_file(stage.join(name));
    }
    let _ = fs::remove_dir(stage);
}
fn export(
    input: &Project,
    output_root: &Path,
    cancel: &AtomicBool,
    export_type: &str,
) -> Result<ExportResult> {
    let mut project = validate_project(input)?;
    ensure!(
        !project.notes.is_empty(),
        "工程还没有音符，请先添加音符再导出。",
    );
    let mut report = project.report.clone().unwrap_or_else(|| json!({}));
    for (key, value) in [
        ("source_notes", json!(project.notes.len())),
        ("removed_polyphony", json!(0)),
        ("dropped_out_of_range", json!(0)),
        ("transpose_semitones", json!(0)),
        ("speed", json!(1.0)),
    ] {
        report.as_object_mut().unwrap().entry(key).or_insert(value);
    }
    let skip_long_rests = match project
        .options
        .as_ref()
        .and_then(|o| o.get("skip_long_rests"))
    {
        None => false,
        Some(v) => v
            .as_bool()
            .ok_or_else(|| anyhow::anyhow!("跳过长空白设置必须是布尔值。"))?,
    };
    let (performance, skipped_rests, removed_rest_seconds) =
        compress_long_rests(&project.notes, skip_long_rests);
    let (events, delayed) = build_events(&performance)?;
    let actual = decode_events(&events)?;
    ensure!(
        actual
            .iter()
            .map(|n| n.pitch)
            .eq(project.notes.iter().map(|n| n.pitch)),
        "按键和音符校验不一致，已中止导出。"
    );
    fs::create_dir_all(output_root)?;
    let output_root = output_root.canonicalize()?;
    check_cancel(cancel)?;
    let folder = output_root.join(format!("{}-{}", safe_label(&project.title), unique_id()));
    let stage = output_root.join(format!(".partial-{}", unique_id()));
    fs::create_dir(&stage)?;
    let result = (|| -> Result<ExportResult> {
        check_cancel(cancel)?;
        let options = project
            .options
            .clone()
            .unwrap_or_else(|| report.get("options").cloned().unwrap_or_else(|| json!({})));
        for (key, value) in [
            ("version", json!(env!("CARGO_PKG_VERSION"))),
            ("melody_notes", json!(project.notes.len())),
            ("current_notes", json!(project.notes.len())),
            ("options", options),
            ("export_type", json!(export_type)),
            ("edited", json!(export_type == "edited_project")),
            ("delayed_notes", json!(delayed)),
            (
                "duration_seconds",
                json!(events.last().unwrap().0 as f64 / 1000.0),
            ),
            ("skip_long_rests", json!(skip_long_rests)),
            ("skipped_long_rests", json!(skipped_rests)),
            (
                "removed_rest_seconds",
                json!((removed_rest_seconds * 1e6).round_ties_even() / 1e6),
            ),
            (
                "preview",
                json!("合成音色；按导出的按键时间表渲染；不是游戏实录。"),
            ),
        ] {
            report[key] = value;
        }
        let template = include_str!("../../assets/player.ahk").trim_start_matches('\u{feff}');
        ensure!(template.contains("__EVENTS__"), "AHK 模板缺少事件占位符。");
        let table = events
            .iter()
            .map(|(ms, key, down)| format!("{ms},{key},{down}"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(
            stage.join("演奏脚本.ahk"),
            format!("\u{feff}{}", template.replace("__EVENTS__", &table)),
        )?;
        write_midi(&actual, &stage.join("口琴单旋律.mid"))?;
        fs::write(stage.join("音符.json"), serde_json::to_vec_pretty(&actual)?)?;
        fs::write(stage.join("按键时间表.json"), serde_json::to_vec(&events)?)?;
        fs::write(
            stage.join("转换报告.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        project.report = Some(report.clone());
        save_project(&stage.join("工程.hstudio"), &project)?;
        render_wav(&events, &stage.join("试听.wav"), cancel, 22050)?;
        let result = prepare_export(folder.clone(), report, project, actual);
        check_cancel(cancel)?;
        fs::rename(&stage, &folder)?;
        Ok(result)
    })();
    if result.is_err() {
        cleanup_staging(&stage, &output_root);
    }
    result
}

#[cfg(test)]
mod output_contract_tests {
    use super::*;
    #[test]
    fn export_filenames_are_stable_canonical_names() {
        // Regression guard: these names are user-visible files and cross-checked
        // by export tests. Any encoding damage here breaks roundtrips silently.
        assert_eq!(
            OUTPUTS,
            [
                "演奏脚本.ahk",
                "口琴单旋律.mid",
                "音符.json",
                "按键时间表.json",
                "转换报告.json",
                "工程.hstudio",
                "试听.wav",
            ]
        );
    }
}
