# 口琴工坊 · Rust / WinUI 3

本项目是口琴工坊的独立 Rust 重构，原项目 D:\HarmonicaStudio 保留。桌面使用真正的 WinUI 3 控件和 Direct2D 画布；业务、MIDI、编辑、导出、试听、遥控服务和诊断使用 Rust。源码、测试、构建与成品均不需要 Python。

当前版本以 Cargo.toml 为准。此版本处于迁移验收阶段；自动测试结果和未完成的人工界面验证见 [验收记录](verification/迁移验收.md)。

## 使用

完成打包后，双击 app/HarmonicaStudio/HarmonicaStudio.exe，或根目录“启动口琴工坊.cmd”。

- 打开 MIDI，选择声部及转换参数，生成曲谱。
- 在“编辑与试听”调整音符、时间、音域及心动片段，试听或开始游戏演奏。
- 右上角齿轮打开设置，手机图标打开遥控连接。
- 曲库仍是文件卡片上的下拉菜单；精简模式隐藏转换界面和编辑工具栏。
- 游戏演奏使用随附的 AutoHotkey v2。F6 开始/停止，F8 退出脚本。

迁移按原 gui、theme、editor、settings_ui、remote_ui 的布局和行为实现。不同窗口大小、系统缩放和弹窗还需要实际渲染对照；编译通过不等于视觉完全一致。

便携版应整体移动，不可只复制 EXE。根目录保留 `HarmonicaStudio.exe`（启动器）、`使用说明.txt`、`samples/`（曲库）、`data/`（个人数据）和 `program/`（内部主程序、运行 DLL、语言资源、assets 与 third_party）。设置支持外部曲库；HARMONICA_STUDIO_HOME 可覆盖个人数据位置。开发调试模式以此仓库根目录为应用根。

启动器保留调用者的参数、工作目录、输出和退出码；默认曲库及个人数据始终相对于外层便携目录。`src/launcher.rs` 由正式打包脚本独立编译，共用主程序的图标与版本资源，不链接 WinUI。请勿单独移动 program 或删除其布局标记。

原 .hstudio 工程可直接打开。Rust 版使用独立 data 目录，不自动修改原项目的曲库、设置或恢复记录。

## 开发

需要 Windows x64、Rust MSVC 工具链、Visual Studio C++ Build Tools 和 Windows SDK。当前机器已在 .tools 下配置独立 Rust 工具链；脚本优先使用它，否则使用 PATH 中的 cargo。

在项目根目录的 PowerShell 中：

    . .\scripts\common.ps1
    $taskCargo = Get-Cargo
    & $taskCargo run --locked -- gui

运行全部自动检查：

    .\scripts\test.ps1

仅测与桌面无关的 Rust 业务逻辑：

    .\scripts\test.ps1 -CoreOnly

CoreOnly 使用 target/core-only 独立目录，且桌面 EXE 在 Cargo 中要求 desktop 功能，不会用无界面的构建覆盖正常程序。

测试窗口只显示中文进度和通过数量。每次运行的完整测试输出及编译诊断分别保存到 `verification/test-日期时间-编号/`，失败时会提示原因和日志位置。

## 打包

双击“打包.cmd”，或：

    .\scripts\build.ps1
    .\scripts\build.ps1 -Version 2.0.0-alpha.1

双击“打包.cmd”会显示当前版本，输入新版本号即可更新，直接回车则保持不变；输入格式错误会提示重新输入。直接运行 `scripts/build.ps1` 默认保持版本且不询问；加 `-PromptVersion` 可显示同样的交互提示，或用 `-Version` 明确指定版本。输入与当前相同的版本不会触发版本同步。

打包工具是 Cargo/Rust 编译器、Windows SDK 资源编译器和 PowerShell 打包脚本。当前生成自包含 WinUI 3 的便携目录，不使用 PyInstaller，不依赖 .NET 应用运行时。

统一入口依次完成源码检查、Rust 测试、构建脚本夹具测试、正式编译、资源及许可证收集、成品自测、受保护安装。失败不发布未通过自测的程序；已存在的 samples 和 data 文件保留。若安装回滚本身失败，会保留恢复文件并报告位置。

升级旧平铺目录时，仅处理与新包清单对应的旧程序文件，以及明确列出的旧图标、旧项目许可证和调试文件，并移除由此变空的目录。内容不同或已废弃的旧文件备份到 `program/previous-layout/`；无法确认的个人文件留在原处。迁移与程序安装共用回滚事务。

打包窗口显示六个阶段，完成后列出用时、程序位置和保留文件数量。完整日志保存到每次独立的 `verification/build-日期时间-编号/`：`rust-tests.log` 为测试详情，`*.stderr.log` 为编译器诊断，`script-tests.log` 为安装保护检查，`portable-smoke.json` 为成品自检，`summary.txt` 为成功摘要；失败记录见 `failure.log`。历史运行日志不会被后一次覆盖。

第一次编译需下载 Rust 依赖和固定版本的微软运行组件。WinUI Rust 绑定固定于微软 windows-rs 提交，最小源码集和本地适配说明在 third_party/windows-rs/SOURCE.md。成品第三方目录含依赖版本与许可证清单。

## 命令行

以下示例在便携目录运行；在 PowerShell 中通过管道读取 JSON：

    .\HarmonicaStudio.exe build-info | Out-String
    .\HarmonicaStudio.exe inspect .\samples\欢乐颂.mid | Out-String
    .\HarmonicaStudio.exe convert .\samples\欢乐颂.mid --out .\data\exports | Out-String
    .\HarmonicaStudio.exe export .\工程.hstudio --out .\data\exports | Out-String
    .\HarmonicaStudio.exe validate .\工程.hstudio | Out-String
    .\HarmonicaStudio.exe self-test | Out-String

可用 convert --help 查看声部、速度、移调、旋律模式和空白处理选项。命令行声道为 1–16，内部为 0–15。导出已编辑工程不会重新提取旋律或再次应用历史移调。

详见 [架构](docs/architecture.md)、[诊断](docs/diagnostics.md)、[第三方组件](THIRD_PARTY.md)。
