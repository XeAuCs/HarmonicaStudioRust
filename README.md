# 口琴工坊 · Rust / WinUI 3

口琴工坊的独立 Rust 重构，用于 MIDI 声部选择、口琴曲谱编辑、电脑试听和游戏演奏。桌面使用 WinUI 3 控件和 Direct2D 画布，手机通过浏览器遥控电脑。业务、MIDI、编辑、导出、试听、遥控服务和诊断使用 Rust，源码、测试、构建与成品均不需要 Python。

当前版本为 **2.1.9**，实际版本以 [Cargo.toml](Cargo.toml) 为准。项目仍处于迁移验收阶段。`verification/` 中的日志和截图仅保存在本地，不随 GitHub 源码分发；自动检查通过不代表所有设备与界面场景都已验收。

## 当前功能

- **声部推荐**：按 0–100 推荐指数排序，默认选择第一项，显示完整音域、音符数量和时长。推荐综合旋律连续性、单音程度、覆盖范围及口琴音域适配等因素，分数不是正确率，仍可手动换声部。
- **中文 MIDI 名称**：优先读取 UTF-8，Windows 下兼容 GB18030（含 GBK/GB2312）；无法解码时显示占位名称。
- **曲库选曲**：搜索曲名或文件名、显示当前曲目与已知时长；刷新保留搜索词，选曲后收起，支持 Esc 关闭弹窗。
- **曲谱编辑与演奏**：调整音符和转换参数、保存工程、导出及电脑试听，通过随附的 AutoHotkey v2 控制游戏演奏。
- **手机遥控**：扫码连接同一局域网中的电脑，选曲、控制播放并查看音轨进度；连续播放采用本地动画和平滑进度校准。
- **主题与便携打包**：提供纸色、绿色、蓝色、梅紫主题；打包默认保持版本，升级保留已有曲库和个人数据。

## 使用

完成打包后，双击 app/HarmonicaStudio/HarmonicaStudio.exe，或根目录“启动口琴工坊.cmd”。

- 打开 MIDI，确认默认推荐声部或手动选择其他声部，调整转换参数并生成曲谱。
- 在“编辑与试听”调整音符、时间、音域及心动片段，试听或开始游戏演奏。
- 右上角齿轮打开设置，手机图标打开遥控连接。
- 文件卡片上的“曲库”打开搜索选曲弹窗；精简模式隐藏转换界面和编辑工具栏。
- 游戏演奏使用随附的 AutoHotkey v2。F6 开始/停止，F8 退出脚本。

迁移按原 gui、theme、editor、settings_ui、remote_ui 的布局和行为实现。不同窗口大小、系统缩放和弹窗还需要实际渲染对照；编译通过不等于视觉完全一致。

便携版应整体移动，不可只复制 EXE。根目录保留 `HarmonicaStudio.exe`（启动器）、`使用说明.txt`、`samples/`（曲库）、`data/`（个人数据）和 `program/`（内部主程序、运行 DLL、语言资源、assets 与 third_party）。设置支持外部曲库；HARMONICA_STUDIO_HOME 可覆盖个人数据位置。开发调试模式以此仓库根目录为应用根。

启动器保留调用者的参数、工作目录、输出和退出码；默认曲库及个人数据始终相对于外层便携目录。`src/launcher.rs` 由正式打包脚本独立编译，共用主程序的图标与版本资源，不链接 WinUI。请勿单独移动 program 或删除其布局标记。

原 .hstudio 工程可直接打开。Rust 版使用独立 data 目录，不自动修改原项目的曲库、设置或恢复记录。

### 手机连接

在电脑右上角打开“手机遥控”，选择手机可访问的网卡地址并扫码。手机与电脑需要处于同一局域网，声音由电脑播放；游戏模式还需要电脑进入游戏口琴界面。重新开启遥控后应重新扫码。连接过期或浏览器无法保留连接凭据时，也需要重新扫码。

手机音轨已针对轮询抖动加入平滑校准，但真实手机帧率、浏览器后台限速和不同网络环境仍需实测，不能把模拟进度测试当作手机流畅度保证。

## 源码目录

| 目录 | 内容 |
| --- | --- |
| `src/core/` | 控制器、后台任务、偏好、路径和诊断 |
| `src/score/` | MIDI、推荐算法、旋律提取、工程与导出 |
| `src/media/` | 试听、演奏与播放时钟 |
| `src/shell/` | WinUI 界面、主题和遥控服务 |
| `assets/` | 图标、演奏脚本及手机网页 |
| `scripts/`、`tests/` | 构建、安装保护和自动检查 |
| `third_party/` | 随项目使用的第三方源码及许可证 |

GitHub 仓库提供源码，不包含本机的 MIDI 曲库、个人工程、`data/`、`app/`、`build/`、`target/` 或验证日志。克隆后请自行将 MIDI 放入曲库，或在设置中选择外部曲库。

## 开发

需要 Windows x64、Rust 1.95 或更新的 MSVC 工具链、Visual Studio C++ Build Tools 和 Windows SDK。脚本优先使用本地 `.tools/cargo/bin/cargo.exe`，否则使用 PATH 中的 cargo；`.tools/` 不包含在仓库中。

在项目根目录的 PowerShell 中：

    . .\scripts\common.ps1
    $taskCargo = Get-Cargo
    & $taskCargo run --locked -- gui

运行全部自动检查：

    .\scripts\test.ps1

仅测与桌面无关的 Rust 业务逻辑：

    .\scripts\test.ps1 -CoreOnly

CoreOnly 使用 target/core-only 独立目录，且桌面 EXE 在 Cargo 中要求 desktop 功能，不会用无界面的构建覆盖正常程序。

默认测试使用代码生成的 MIDI 夹具，不依赖 `samples/` 中的个人曲谱或 `catalog.json`。完整测试输出中的 4 项忽略测试属于可选历史曲库审计，不计为通过；`-CoreOnly` 不包含该审计目标。

如已自行准备完整历史曲库，可显式运行审计（普通用户和打包不需要）：

```powershell
. .\scripts\common.ps1
$taskCargo = Get-Cargo
$env:HARMONICA_TEST_CORPUS = 'D:\自己的历史曲库'
& $taskCargo test --locked --test local_corpus -- --ignored --nocapture
Remove-Item Env:HARMONICA_TEST_CORPUS
```

审计保留固定文件、SHA-256 和历史算法统计校验，任意曲库不能替代该历史数据集；未指定路径、缺少资源或固定哈希不符时会失败，不会因资源缺失直接跳过。

测试窗口只显示中文进度和通过数量。每次运行的完整测试输出及编译诊断分别保存到 `verification/test-日期时间-编号/`，失败时会提示原因和日志位置。

## 打包

双击“打包.cmd”，或：

    .\scripts\build.ps1
    .\scripts\build.ps1 -Version 2.1.7

双击“打包.cmd”会显示当前版本，输入新版本号即可更新，直接回车则保持不变；输入格式错误会提示重新输入。直接运行 `scripts/build.ps1` 默认保持版本且不询问；加 `-PromptVersion` 可显示同样的交互提示，或用 `-Version` 明确指定版本。输入与当前相同的版本不会触发版本同步。

打包工具是 Cargo/Rust 编译器、Windows SDK 资源编译器和 PowerShell 打包脚本。当前生成自包含 WinUI 3 的便携目录，不使用 PyInstaller，不依赖 .NET 应用运行时。

统一入口依次完成源码检查、Rust 测试、构建脚本夹具测试、正式编译、资源及许可证收集、成品自测、受保护安装。失败不发布未通过自测的程序；已存在的 samples 和 data 文件保留。若安装回滚本身失败，会保留恢复文件并报告位置。

正式版正在运行或暂不安装时，先输出到独立目录：

```powershell
.\scripts\build.ps1 -DistPath .\build\preview-release
.\scripts\smoke.ps1 -Path .\build\preview-release\HarmonicaStudio
```

`-DistPath` 指定父目录，脚本在其中生成 `HarmonicaStudio/`。安装到正式目录前请保存工程并正常关闭程序；不要直接覆盖运行中的文件。

升级旧平铺目录时，仅处理与新包清单对应的旧程序文件，以及明确列出的旧图标、旧项目许可证和调试文件，并移除由此变空的目录。内容不同或已废弃的旧文件备份到 `program/previous-layout/`；无法确认的个人文件留在原处。迁移与程序安装共用回滚事务。

版本输入完成后，打包窗口只显示六个中文阶段与检查结果，不显示进度条或定时耗时提示。完成后列出总用时、程序位置和保留文件数量。完整日志保存到每次独立的 `verification/build-日期时间-编号/`：`rust-tests.log` 为测试详情，`*.stderr.log` 为编译器诊断，`script-tests.log` 为安装保护检查，`portable-smoke.json` 为成品自检，`summary.txt` 为成功摘要；失败记录见 `failure.log`。历史运行日志不会被后一次覆盖。

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
