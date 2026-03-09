# 图标生成指南 / Icon Generation Guide

## 为什么任务栏图标会模糊？

Windows 任务栏图标通常只有 24–48px（高 DPI 下可能到 64px）。如果源图分辨率低，或者主体在画布中占比太小（周围留白过多），缩小后有效像素就会非常少，导致模糊。

**关键原则：图标主体应尽量填满画布，源图至少 1024×1024。**

## 如何更新图标

### 1. 准备源文件

- 使用 `Logo/` 目录下的 SVG 源文件
- 如果主体太小，需要调整 SVG 的 `viewBox` 来裁剪放大：

```xml
<!-- 原始：主体只占画布 37% -->
<svg viewBox="0 0 1024 1024" ...>

<!-- 放大：裁剪到主体区域，留 10% 边距 -->
<svg viewBox="244.4 244.4 535.2 535.2" ...>
```

### 2. 渲染为 1024×1024 PNG

```bash
npx sharp-cli -i Logo/icon-zoomed.svg -o memori-desktop/icons/icon.png resize 1024 1024 --fit contain --background "#0D0D0C"
```

### 3. 生成全平台图标

在 `memori-desktop/` 目录下执行：

```bash
npx tauri icon icons/icon.png
```

这会自动生成：
- `icon.ico` — Windows（含 16/24/32/48/64/256px）
- `icon.icns` — macOS
- `icon.png`, `32x32.png`, `64x64.png`, `128x128.png` 等 — Linux / 通用
- `ios/`, `android/` — 移动端图标
- `Square*.png`, `StoreLogo.png` — Windows Store (Appx)

### 4. 无边框窗口的任务栏图标

本项目使用 `decorations: false`（无边框窗口）。Tauri v2 在此模式下**不会自动**将 bundle 图标应用到任务栏。

解决方案已在 `src/lib.rs` 的 `setup` 中实现，通过 `include_bytes!` 内嵌 `icon.png` 并调用 `set_icon()`：

```rust
let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png"))
    .expect("failed to load icon.png");
main_window.set_icon(icon).expect("failed to set window icon");
```

需要在 `Cargo.toml` 中启用 `image-png` feature：

```toml
tauri = { version = "2.0.6", features = ["image-png"] }
```

### 5. 验证

重新构建应用后，检查任务栏、桌面快捷方式、窗口标题栏的图标是否清晰。
