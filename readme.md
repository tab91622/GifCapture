# recordTogif

`recordTogif` 是一个使用 Rust 开发的 macOS 桌面工具，提供区域录制并导出 GIF。

当前版本为 MVP，技术栈：

- GUI：`iced`
- 录屏：macOS `screencapture` 命令抓帧（后续可替换为 `ScreenCaptureKit`）
- 编码：`gif` crate

## 运行环境

- macOS
- Rust stable（建议 1.80+）

## 本地运行

```bash
cargo run
```

## 使用说明

1. 拖动窗口会实时更新录制区域的 `x/y`，缩放窗口会实时更新 `width/height`
2. 坐标语义为**窗口内容区**（不含标题栏）
3. 你也可以手动输入 `x/y/width/height`，下次拖动/缩放窗口会再次覆盖为窗口联动值
4. 设置 `FPS`（建议 5~12）
5. 设置录制时长秒数（`1~300`，到时自动停止）
6. 点击“Start”开始录制
7. 点击“Stop”停止录制（或等待自动停止）
8. 点击“Export GIF”并选择保存路径

## 交互与样式优化

- 窗口启用透明背景，主内容区域为半透明浮层
- 按钮采用 macOS 极简风格：圆角、轻边框、半透明底色
- 按钮禁用态会自动降低对比度，便于识别当前可操作步骤

## 打包为 dmg（安装包）

先准备一个图标文件：`assets/icon.icns`。

推荐使用项目内脚本：
先安装 `cargo-bundle`

```bash
cargo install cargo-bundle
```

```bash
chmod +x scripts/package_dmg.sh
./scripts/package_dmg.sh
```

手动流程：

```bash
# 1) 生成 .app（依赖 cargo-bundle）
cargo install cargo-bundle
cargo bundle --release

# 2) 使用 create-dmg 或 hdiutil 把 .app 封装成 .dmg
```

## 已知限制

- 当前是“轮询截图抓帧”方案，性能不如原生视频流采集
- 仅支持 macOS
- 没有内置区域选择蒙层（需要手动输入坐标）
