# 第三方组件与资源

本重构不分发 Python 或 Qt。保留原图标、手机页面、AutoHotkey 模板和曲库来源记录；应用原创部分许可见 LICENSE。

## Rust / WinUI

- 微软 windows-rs：固定提交 a38689a8b29520db84e3d398e4389d48f18e6346，MIT OR Apache-2.0。保留依赖源码及两个许可证，窄范围的界面适配记于 third_party/windows-rs/SOURCE.md。
- 来源：https://github.com/microsoft/windows-rs
- Windows App SDK Runtime 2.5.1：按微软包内许可证分发，包含 WinUI 3 运行组件。
- Microsoft.Web.WebView2 1.0.4078.44：随绑定构建助手带入的原生桥接 DLL，遵循包内许可证。
- 其他 Rust 依赖及精确版本以 Cargo.lock 为准。打包时从本机 Cargo 元数据和依赖原件收集许可证，输出 third_party/licenses/dependencies.json。
- 两个微软运行包的原始许可证、NOTICE 和 NuGet 说明文件在打包时收集至 third_party/licenses；runtime-dependencies.json 记录精确包版本与文件来源。

## 编译运行库

Windows x64 MSVC 构建静态链接应用所需的 C 运行库，避免便携版单独依赖 VCRUNTIME140.dll。WinUI 组件仍按微软运行包原件分发。最终依赖检查记录见 verification。

## AutoHotkey

便携版随附 AutoHotkey v2 x64，验证版本 2.0.28，位于 third_party/AutoHotkey/AutoHotkey64.exe。其 GPL-2.0 许可证原件保留在同目录。它作为独立进程执行 AHK 演奏脚本。

- 官方：https://www.autohotkey.com/
- 上游源码及版本：https://github.com/AutoHotkey/AutoHotkey/tree/v2.0.28
- 官方下载：https://www.autohotkey.com/download/ahk-v2.zip

## 曲库、图标与页面

原曲库中的每首曲目及适用范围见 samples/来源说明.md，打包保留该文件及相关来源记录。迁移不变更曲目的权利归属或分发范围。

assets/studio.ico 及手机页面、演奏模板来自原口琴工坊项目，保留原项目许可。