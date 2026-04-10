# ScreenCaptureKit 替换可行性评估

## 结论

可行，且值得作为 macOS 高性能路线推进。建议采用“保留旧实现 + 新后端渐进接入”的方式，避免一次性重构风险。

## 现状与问题

- 当前采集路径：`screencapture` 轮询截图。
- 主要问题：
  - 高 FPS 下系统调用开销明显。
  - 帧时间抖动较大，容易触发掉 Tick。
  - 无法天然获得连续视频帧时间戳。

## ScreenCaptureKit 预期收益

- 更低延迟与更稳定帧间隔。
- 更高上限 FPS 与更低 CPU 占用。
- 可扩展到窗口采集、显示器采集、光标控制等能力。

## 集成建议

1. 在 `capture` 层引入后端抽象：
   - `CaptureBackend::capture_frame(region)`.
2. 保留 `screencapture` 为默认后端，新增 `screen_capture_kit` 后端（feature gate）。
3. 在 `app` 中不感知后端，只处理统一 `CapturedFrame`。
4. 增加运行时回退：
   - 新后端初始化失败时自动降级到旧后端。

## 改造成本（粗估）

- 代码改造：中等（采集层重构 + FFI/桥接）。
- 风险：中等偏高（权限、坐标系、线程模型）。
- 测试成本：中等（多显示器、Retina、不同 macOS 版本）。

## 推荐里程碑

- M1：后端抽象 + 旧后端无行为变更（低风险）。
- M2：接入 ScreenCaptureKit 基础帧流，完成单显示器验证。
- M3：多显示器与权限异常处理完善，默认启用新后端。
