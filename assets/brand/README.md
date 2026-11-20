# ShrimpAPI 品牌素材

`source.png` 为用户提供的原始 `ShrimpAPI.png`，1983 × 793，保留透明背景、配色和字样。

- `logo.png`：从原图 `(160, 145, 1660, 500)` 裁切完整标识，移除外侧多余留白，用于界面侧栏的白色底板。
- `mark.png`：从原图 `(160, 145, 470, 500)` 裁切左侧虾形与双向箭头，避开右侧文字。
- `icon.png`：将 `mark.png` 等比放在白色圆角底板上，输出 1024 × 1024 应用图标，外侧透明。
- `AppIcon.icns`：由 `icon.png` 生成的 macOS 图标，包含从 16 到 1024 像素的普通及 Retina 尺寸。

在 macOS 重新生成：

```sh
swift scripts/generate-brand-assets.swift
sh scripts/generate-macos-icon.sh
```

界面 Logo 和窗口图标编译进可执行文件，应用包通过 `CFBundleIconFile` 使用 `AppIcon.icns`。运行或安装不依赖原始图片所在的下载目录。
