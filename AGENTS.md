# 口琴工坊 Rust / WinUI 3 项目操作指南

本指南适用于 `D:\HarmonicaStudioRust`。这是口琴工坊的 Rust / WinUI 3 重构项目；`D:\HarmonicaStudio` 只作只读对照，不修改其源码、便携版和用户数据。

## 工作边界

- 默认用中文沟通，保留中文界面文字和 UTF-8 编码。
- 修改前先读相关源码、测试和文档，以当前代码为准；不把旧报告、旧截图或旧版本号当作验证结果。
- 不覆盖用户未提交的修改，不强制结束正在运行的程序，不删除曲库、设置、恢复记录、导出和个人工程。
- 项目源码、测试、诊断、构建和打包不依赖 Python；使用 Rust、PowerShell、CMD 和现有 Windows 工具。

## 目录速查

| 路径 | 职责 |
| --- | --- |
| `src/core/` | 控制层、后台任务、路径、偏好和诊断 |
| `src/score/` | MIDI、音符、曲谱编辑、旋律提取、工程和导出 |
| `src/media/` | 试听、播放、传输时钟和播放预览 |
| `src/shell/` | WinUI 3 界面、主题和手机遥控 |
| `tests/` | Rust 合约和集成测试 |
| `scripts/` | 测试、构建、打包、成品检查和许可证收集 |
| `assets/`、`samples/` | 运行资源和种子曲库 |
| `third_party/` | 随程序分发的组件及许可证来源 |
| `app/HarmonicaStudio/` | 正式便携版，不是源码目录 |
| `build/`、`target/`、`data/` | 构建暂存、Cargo 输出和源码运行数据，不纳入交付 |
| `verification/` | 构建日志、自检结果和必要截图 |

`src/lib.rs` 用 `#[path]` 将物理目录映射为现有逻辑模块；修改时以实际文件位置为准，不新建旧布局的重复文件。

## 架构约束

- `AppController`（`src/core/controller.rs`）是业务状态和变更的唯一入口；WinUI 与 `src/shell/remote.rs` 不得各自实现业务流程。
- `src/core/jobs.rs` 的后台结果必须检查任务身份、工程身份、编辑版本和取消状态；工作线程不直接修改 UI 或应用状态，关闭时不能用阻塞等待代替生命周期管理。
- MIDI 原始解析保留多声部、超出口琴音域的音符和原始时间；可编辑曲谱统一走现有音符规范化逻辑。
- 曲谱时间是源数据。按键提前量、松键间隔、长空白压缩等只在播放/调度模块派生，不写回并累积改变曲谱。
- 旋律提取产生的八度调整属于提取历史；编辑或重新导出不得重复应用。
- 保存、发布和便携版安装使用现有暂存、校验、原子替换和回滚流程，失败时保护已有程序和用户数据。

## WinUI 3 与主题

- `src/shell/theme.rs` 保存 `Palette`；`src/shell/gui.rs` 负责 WinUI 资源覆盖和 Direct2D Canvas 绘制。新增可见颜色先考虑扩展 `Palette`，不要依赖 WinUI 默认强调色或在控件中散落固定蓝/紫色。
- 主题切换必须同时检查原生控件、弹出层、对话框、曲谱 Canvas、时间轴、钢琴键、图标、进度条和二维码；共享 Palette 更新后需要让相关 Canvas 重绘。
- 资源映射或静态测试通过，不等于主题问题已修复。当前迁移验收仍缺真实窗口的视觉对照；涉及界面/主题的改动必须在 Windows 实际运行，至少检查默认主题和所有受影响主题、窗口尺寸及系统缩放，并把截图或未验证项写入 `verification/`。
- 不要把猜测性的 WinUI 资源键、原生类型构造或临时诊断开关当作稳定方案；先用最小复现确认，再保留可解释且有测试/实际窗口证据的改动。

## 常用命令

在项目根目录 PowerShell 中执行。脚本优先使用 `.tools\cargo\bin\cargo.exe`，否则使用 PATH 中的 Rust MSVC 工具链。

```powershell
.\scripts\test.ps1
.\scripts\test.ps1 -CoreOnly
.\scripts\build.ps1
.\scripts\smoke.ps1
```

开发运行可使用：

```powershell
. .\scripts\common.ps1
$taskCargo = Get-Cargo
& $taskCargo run --locked -- gui
```

`scripts\build.ps1` 是正式发布的唯一入口：它串行执行测试、构建、许可证收集、成品自测和安全安装。首次构建可能准备固定版本的 Windows App SDK / WebView2 运行依赖；不要绕过现有缓存校验和许可证检查，也不要手工拼接发布流程。

便携版必须整体移动，至少保留 `HarmonicaStudio.exe`、运行 DLL、`assets/`、`samples/`、`third_party/`、`data/` 和使用说明。默认曲库、个人数据位于 EXE 旁；显式外部曲库和 `HARMONICA_STUDIO_HOME` 覆盖必须继续有效。安装时保留已有 `samples/`、`data/` 及用户文件；程序运行时先让用户保存并正常关闭。

## 验证与交付

- 先跑受影响的测试；涉及界面、主题、播放、保存、后台任务或打包时，再跑 `scripts\test.ps1`，必要时运行 `scripts\build.ps1` 和 `scripts\smoke.ps1`。
- Rust 测试、脚本夹具、成品自检和真实 WinUI 截图是不同证据，跳过或未执行不能写成通过。测试输入、音频、网络和游戏控制使用隔离夹具或假后端，不向真实游戏发送按键。
- 不把用户曲谱内容和敏感路径写入公开日志。生成物、缓存和用户数据不加入源码交付；构建失败或成品自检失败时，不得把旧便携版当作新版本。
- 递归删除或移动前解析绝对路径，确认目标位于本项目拥有的目录内并拒绝文件系统链接；优先使用现有脚本和 `-LiteralPath`。
- 交付说明写清问题、实际行为、执行过的验证命令、截图位置以及尚未验证的限制；只报告已经完成的检查。
