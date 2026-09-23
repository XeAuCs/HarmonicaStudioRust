# 选歌卡顿日志

从 2.1.20 起，桌面每次启动会在个人数据目录的 `logs/` 下创建独立的 `performance-*.jsonl` 文件。正式便携版默认位置为 `app/HarmonicaStudio/data/logs/`；预览包使用自身外层的 `data/logs/`。显式 `HARMONICA_STUDIO_HOME` 覆盖仍按原有路径规则生效。

复现一次手机选歌停顿后，提供本次启动对应的日志及大致发生时间。日志只在本机保存，不自动上传，也不记录曲名、曲谱内容、文件路径、IP 或遥控凭据。旧版没有这些耗时记录，升级前的停顿无法追溯。

每行是 JSON，`unix_ms` 是事件的 Unix 毫秒时间，`duration_ms` 是单调时钟测得的耗时。`span` 配对同一阶段的 `begin` 和 `end`；`job` 关联同一个后台任务及其子阶段。`elapsed` 是一次直接测量。`end` 表示作用域结束，异常返回或取消也会结束作用域，不等于业务成功。

| 阶段 | 说明 |
| --- | --- |
| `remote.command_reply` | 完整手机指令已解析后，到控制器回复或超时；不包含手机到服务器的网络传输 |
| `remote.command_queue` | 指令在电脑端排队等待控制器处理 |
| `remote.command_timeout` | 控制器未在 3 秒内回复 |
| `selection.request` | 发起选歌，包括提交原工程保护保存 |
| `selection.save_wait` | 切换操作等待保存完成；退出等保护流程也使用该记录 |
| `selection.stop_previous` | 关闭上一首的试听和演奏 |
| `job.queue` | 后台线程从提交到开始执行 |
| `job.load` / `job.convert` / `job.export` | 对应后台任务的整体耗时 |
| `selection.restore_project` | 查找和读取该歌曲的自动保存工程 |
| `midi.parse` / `midi.rank_parts` | MIDI 读取解析 / 声部推荐排序 |
| `convert.parse` / `convert.extract_melody` | 转换时读取解析 / 旋律提取 |
| `convert.build_project` | 来源校验与工程建立 |
| `export.build_events` / `export.write_files` | 建立播放事件 / 写入工程、MIDI 等文件 |
| `preview.render_wav` | 生成试听音频 |
| `export.commit` | 建立播放映射并提交暂存导出 |
| `job.result_wait` | 后台已完成，等待控制器领取结果 |
| `job.apply_result` / `selection.install_result` | 控制器应用后台结果 / 安装准备好的曲谱 |
| `preview.open_audio` | 打开试听音频并定位 |
| `controller.poll_gap` | 两次控制器轮询间隔至少 500 毫秒 |
| `controller.poll_slow` / `ui.message_slow` | 控制器或界面消息处理至少 50 毫秒 |
| `ui.timer.fire_late.visible/minimized/hidden/unknown` | UI 定时器比计划时间晚触发至少 250 毫秒；后缀是触发时的窗口状态 |
| `ui.timer.message_wait` | 定时器已触发，但 Tick 消息至少 50 毫秒后才进入界面处理 |
| `ui.timer.schedule_error` | 界面定时器未能安排下一次 Tick |
| `process.heartbeat_gap` | 独立后台心跳线程超过 500 毫秒未运行，提示整个进程或系统也可能暂停 |
| `remote.publish_slow` | 构建并发布遥控状态至少 50 毫秒 |
| `save.write` | 后台写入工程保存文件 |

先看明显偏大的 `duration_ms`。若 `job.queue` 大，线程开始运行慢；若 `job.result_wait` 大，后台完成后界面没有及时领取结果；若 `preview.render_wav` 大，时间主要花在生成试听。多层阶段会相互包含，不能把全部耗时简单相加。`controller.poll_gap` 只能表明控制器没有及时获得调用，不能单凭它断定是 CPU、GPU 或具体系统调度问题。

排查长时间轮询间隔时，对照同一时间的三类新记录：`ui.timer.fire_late.*` 表示 WinUI 定时器回调本身晚到，`ui.timer.message_wait` 表示回调已执行但消息晚进入组件，`process.heartbeat_gap` 表示独立工作线程也有长时间未运行。若只看到定时器迟到，应继续检查 UI 调度和窗口状态；若后台心跳也迟到，应检查系统或进程级停顿。没有某条记录不等于该环节绝对正常：阈值以下不写入，异常退出可能遗失末尾记录，也可能是定时器没有触发。窗口状态只描述回调发生时的状态，不能证明整个间隔内的状态。新记录从 2.1.22 开始提供，旧日志不能反推这些信息。

写盘由独立线程完成，业务线程只尝试投递到最多 512 条的队列，队列满时丢弃记录而不等待磁盘。写线程逐条确认写盘，正常关闭时最多等待 2 秒排空队列并确认落盘；异常退出、写盘超时或磁盘故障仍可能缺少记录。`dropped_records` 表示累计丢弃数。每个文件最多约 4 MiB，达到上限后记录 `log_limit` 并停止本次记录，重新启动会创建新日志；旧日志不自动删除。此日志不测量手机浏览器绘制、真实网络往返或实际 GPU 帧率。
