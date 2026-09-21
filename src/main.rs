#![cfg_attr(windows, windows_subsystem = "windows")]
use anyhow::Result;
use clap::{Parser, Subcommand};
use harmonica_studio::{diagnostics, melody, midi, models::Options, project, service};
use std::{path::PathBuf, sync::atomic::AtomicBool};

#[derive(Parser)]
#[command(name = "HarmonicaStudio", version, about = "口琴工坊 · Rust / WinUI 3")]
struct Cli {
    #[arg(long, global = true, help = "启动手机遥控")]
    remote: bool,
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    /// 打开桌面界面
    Gui {
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// 查看 MIDI 声部与推荐分数
    Inspect {
        file: PathBuf,
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        transpose: i32,
        #[arg(long)]
        no_auto_octave: bool,
        #[arg(long)]
        phrase_octave: bool,
        #[arg(long, default_value = "sustain", value_parser = ["sustain", "highest", "continuous"])]
        mode: String,
    },
    /// 提取旋律并导出工程、MIDI、AHK 与 WAV
    Convert {
        file: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long, default_value_t = 1.0)]
        speed: f64,
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        transpose: i32,
        #[arg(long)]
        track: Option<usize>,
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=16))]
        channel: Option<u8>,
        #[arg(long, default_value = "sustain", value_parser = ["sustain","highest","continuous"])]
        mode: String,
        #[arg(long)]
        keep_silence: bool,
        #[arg(long)]
        no_auto_octave: bool,
        #[arg(long)]
        phrase_octave: bool,
        #[arg(long)]
        skip_long_rests: bool,
    },
    /// 导出已编辑工程；不会重新提取或叠加移调
    Export {
        file: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// 检查工程格式和音符时间
    Validate { file: PathBuf },
    /// 无声、无按键的成品自测
    SelfTest,
    /// 查看编译功能，供成品检查验证桌面能力
    BuildInfo,
    /// 使用临时合成曲谱测试真实控制层，输出 JSON
    Diagnose {
        #[arg(long, default_value_t = 1000, value_parser = clap::value_parser!(u32).range(1..=100000))]
        notes: u32,
        #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u32).range(1..=100))]
        repeat: u32,
        #[arg(long, default_value = "save", value_parser = ["save","export","edit","load","library"])]
        scenario: String,
        #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(1..=600))]
        timeout: u64,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}
fn run(cli: Cli) -> Result<()> {
    let cancel = AtomicBool::new(false);
    let output = match cli.command {
        None | Some(Command::Gui { .. }) => {
            let file = if let Some(Command::Gui { file }) = cli.command {
                file
            } else {
                None
            };
            #[cfg(all(windows, feature = "desktop"))]
            return harmonica_studio::gui::run(file, cli.remote);
            #[cfg(not(all(windows, feature = "desktop")))]
            {
                let _ = file;
                anyhow::bail!("此构建未启用 Windows 桌面，请使用默认 desktop 功能编译。");
            }
        }
        Some(Command::Inspect {
            file,
            transpose,
            no_auto_octave,
            phrase_octave,
            mode,
        }) => {
            let (parts, names) = midi::read_midi(&file)?;
            let options = Options {
                transpose,
                auto_octave: !no_auto_octave,
                phrase_octave,
                melody_mode: mode,
                ..Options::default()
            };
            options.validate()?;
            let mut rows = Vec::new();
            for ((track, channel), score) in melody::rank_parts(&parts, &names) {
                let (_, fit) = melody::fit_part(&parts[&(track, channel)], &options)?;
                rows.push(serde_json::json!({"track":track,"channel":channel+1,"name":names.get(&track).cloned().unwrap_or_default(),"notes":parts[&(track,channel)].len(),"recommendation_score":score,"fit":fit,"fit_options":options}));
            }
            serde_json::Value::Array(rows)
        }
        Some(Command::Convert {
            file,
            out,
            speed,
            transpose,
            track,
            channel,
            mode,
            keep_silence,
            no_auto_octave,
            phrase_octave,
            skip_long_rests,
        }) => {
            let options = Options {
                speed,
                transpose,
                track,
                channel: channel.map(|n| n - 1),
                melody_mode: mode,
                trim_silence: !keep_silence,
                auto_octave: !no_auto_octave,
                phrase_octave,
                skip_long_rests,
            };
            let out = out.unwrap_or_else(|| harmonica_studio::paths::data_root().join("exports"));
            let result = service::convert(&file, &out, &options, &cancel)?;
            serde_json::json!({"folder":result.folder,"report":result.report})
        }
        Some(Command::Export { file, out }) => {
            let project = project::load_project(&file)?;
            let out = out.unwrap_or_else(|| harmonica_studio::paths::data_root().join("exports"));
            let result = service::export_project(&project, &out, &cancel)?;
            serde_json::json!({"folder":result.folder,"report":result.report})
        }
        Some(Command::Validate { file }) => {
            let project = project::load_project(&file)?;
            serde_json::json!({"ok":true,"schema_version":project.schema_version,"title":project.title,"notes":project.notes.len()})
        }
        Some(Command::SelfTest) => diagnostics::self_test()?,
        Some(Command::BuildInfo) => {
            serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"desktop_enabled":cfg!(all(windows,feature="desktop")),"executable":std::env::current_exe()?,
                "application_root": harmonica_studio::paths::application_root(),
                "resource_root": harmonica_studio::paths::resource_root(),
                "data_root": harmonica_studio::paths::data_root(),
                "default_library_root": harmonica_studio::paths::default_library_root()})
        }
        Some(Command::Diagnose {
            notes,
            repeat,
            scenario,
            timeout,
            output,
        }) => {
            let result = diagnostics::diagnose(notes, repeat, &scenario, timeout)?;
            if let Some(path) = output {
                if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(path, serde_json::to_vec_pretty(&result)?)?;
            }
            result
        }
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
fn main() {
    #[cfg(windows)]
    if std::env::args_os().len() > 1 {
        unsafe extern "system" {
            fn AttachConsole(process_id: u32) -> i32;
        }
        // Attach only to an existing caller console; no extra console for desktop launch.
        unsafe {
            AttachConsole(u32::MAX);
        }
    }
    let cli = Cli::parse();
    let is_gui = matches!(&cli.command, None | Some(Command::Gui { .. }));
    if let Err(error) = run(cli) {
        eprintln!("操作失败：{error:#}");
        if is_gui {
            let logs = harmonica_studio::paths::data_root().join("logs");
            let _ = std::fs::create_dir_all(&logs);
            let _ = std::fs::write(logs.join("startup-error.log"), format!("{error:#}"));
            #[cfg(windows)]
            {
                #[link(name = "user32")]
                unsafe extern "system" {
                    fn MessageBoxW(
                        hwnd: isize,
                        text: *const u16,
                        title: *const u16,
                        flags: u32,
                    ) -> i32;
                }
                let text: Vec<u16> = format!("无法启动口琴工坊：{error:#}")
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                let title: Vec<u16> = "口琴工坊".encode_utf16().chain(Some(0)).collect();
                unsafe {
                    MessageBoxW(0, text.as_ptr(), title.as_ptr(), 0x10);
                }
            }
        }
        std::process::exit(1);
    }
}
