# Rust 性能诊断

diagnose 使用临时合成工程、真实 AppController、无声试听后端和无按键游戏后端。不会读取或修改 HARMONICA_STUDIO_HOME 指向的个人数据。不会向游戏发送按键。

运行示例（在便携目录）：

    .\HarmonicaStudio.exe diagnose --scenario save --notes 10000 --repeat 3 --timeout 60 --output 保存诊断.json | Out-String
    .\HarmonicaStudio.exe diagnose --scenario export --notes 1000 --repeat 3 --timeout 60 --output 导出诊断.json | Out-String

场景为 save、export、edit、load、library。library 仅扫描临时生成的 8 份 MIDI，每份包含指定数量的音符；不扫描默认曲库。音符规模范围 1–100000，重复次数 1–100，超时范围 1–600 秒。合成音符总时长在工程允许范围内。

JSON 包含 schema_version、ok、version、scenario、notes、repeat、display_backend、audio_backend、game_input、samples、mean_completed_ms。

每次样本的 dispatch_ms 是控制层调用返回耗时；completed_ms 是提交至任务完成的总耗时，包含结果轮询等待。两者均不是界面响应时间。此实现没有单独上报后台纯计算耗时，不可从差值直接推断其数值。

对比性能时使用相同构建类型、机器、规模、场景与重复次数串行执行，记录运行环境和缓存条件。检查进程退出码和 JSON 的 ok，超时、错误及缺失样本不算通过。不能将不同实现、显示后端或不受控缓存的少量数据宣传为确定的提速比例。

self-test 检查工程往返、原谱时间与标记不变、MIDI/WAV/事件导出一致、真实控制层保存和 AHK --validate 无按键校验。scripts/smoke.ps1 还检查 desktop 编译功能、运行文件与许可证，防止将不带桌面的构建作为成品。

实际窗口渲染、系统缩放、音频听感和真实游戏演奏需要单独人工验收。