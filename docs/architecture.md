# Rust 应用架构

## 状态与入口

大文件按职责拆成私有子模块，原有逻辑模块路径保持不变：

- `shell/gui.rs` 保留桌面组件、状态同步和启动入口；`shell/gui/` 分别组织事件、主布局、导入页、编辑页、对话框、主题资源、控件、键盘、绘图、Windows 接口及测试。`Component::update/view` 委托给内部方法，事件后的同步和重绘顺序不变。
- `core/controller.rs` 保留唯一的 `AppController`、后台结果轮询和关闭生命周期；`core/controller/` 组织状态类型、工程保存、转换导出、播放、遥控快照及测试。子模块仍操作同一个控制器，不引入第二个状态所有者。
- `score/editor.rs` 保留编辑模型；`score/editor/` 放置编辑、播放保护和视图合约测试。
- `tests/core_contracts.rs` 保留同名集成测试入口，`tests/core_contracts/` 按领域拆分测试及共享夹具；`scripts/test.ps1 -CoreOnly` 继续运行完整的核心合约测试。

main.rs 提供桌面和命令行入口，gui.rs 将原桌面源码的布局、主题和交互映射为 WinUI 3。文本、按钮、输入和页签使用 WinUI 原生元素；设置与手机连接采用原生 ContentDialog 管理模态焦点；钢琴卷帘、图标和时间条由 windows-canvas/Direct2D 绘制。业务无需 Qt 或解释器。

controller.rs 的 AppController 是唯一业务状态所有者。桌面和 remote.rs 的 HTTP 命令都调用同一控制层。后台线程不修改控件或状态，只发送结果；主线程 poll 处理结果并更新视图。接收结果同时检查任务身份、工程身份和编辑版本。

jobs.rs 管理取消、后台结果和串行保存队列。显式保存按顺序处理；仅相邻可合并的自动保存被压缩。快照独立于后续编辑。关闭或换曲先完成需要的保存；取消后台任务后，仍等待线程真正返回才完成关闭。主线程不以 sleep 或同步 join 代替生命周期管理。

## 数据处理

- models.rs、notes.rs：音符和转换选项；可编辑音符统一验证。
- midi.rs：完整 MIDI 声部读取、节奏时间换算和写出；原始解析不按口琴音域删音。
- melody.rs：声部排序、旋律提取、速度、移调、音域及乐句八度。
- project.rs：与原 .hstudio 数据格式兼容的工程读写、校验和安全替换。
- service.rs：组织转换及完整导出，发布前校验临时结果。
- schedule.rs、rests.rs：派生按键计划和压缩空白的时间映射；不改变原工程时间。
- preview.rs：WAV 合成；playback.rs：Windows MCI 试听、独立 AutoHotkey 演奏及测试后端。
- transport.rs：设备时钟同步及两次采样间的位置估计。
- editor.rs：无界面依赖的命中、缩放、滚动、选择、拖动、撤销重做；提交时复用统一校验。

已编辑工程的重新导出不会再跑提取、不会叠加历史移调。标记保存原谱时间，播放和手机展示使用对应时间映射。

## 曲库、设置与遥控

library.rs 后台扫描曲库；song_projects.rs 管理歌曲对应工程。文件系统通知触发刷新并去抖，不逐帧扫描曲库。preferences.rs 保存主题、模式、曲库位置及转换选项。

paths.rs 统一资源和数据路径：正式版以 EXE 所在目录为准，调试版以仓库根目录为准。默认曲库 samples，默认个人目录 data；支持显式外部路径及 HARMONICA_STUDIO_HOME。

remote.rs 提供原手机 HTML/CSS/JS 页面及兼容命令。Windows 系统随机源生成会话令牌；请求检查令牌、Host 和 Origin，限制连接数、请求体和命令队列。网络线程只提交命令和读取发布快照。曲谱快照按工程及版本缓存，状态定时发布，避免每帧重复序列化大谱。

## 构建与验证边界

Cargo 默认 desktop 功能；桌面命名的二进制明确要求 desktop。纯业务测试使用独立 target/core-only，不覆盖桌面 EXE。build.rs 校验 WinUI 运行组件并嵌入图标和版本资源。

scripts/build.ps1 是唯一发布入口，串行测试、构建、成品自测及安装。文件锁或安装失败时回滚，回滚失败保留恢复文件；不会强制结束用户程序。曲库与个人数据既有文件保留。

诊断使用真实控制层和假的音频、游戏后端；不测量真实游戏兼容性。源码布局对应、自动逻辑测试和实际界面截图对照是不同验收项，不能互相替代。
