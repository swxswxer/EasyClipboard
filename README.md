# EasyClipboard 0.1.0

本地优先的 macOS 菜单栏剪贴板历史 MVP，支持 macOS 13+ Apple Silicon。

## MVP 功能

- 文本、PNG/JPEG/TIFF 图片和多文件剪贴板历史
- 搜索、固定、删除、自定义分组和分组永久保留
- `⌘⇧V` 全局快捷键与鼠标所在显示器底部弹窗
- 辅助功能授权后自动切回目标应用并粘贴
- 本地 SQLite/FTS5、数量/天数保留规则和排除应用列表
- 菜单栏常驻、暂停记录、登录时启动和单实例运行

账号、云同步、网络请求、自动更新、Intel 和 Windows 不在 0.1.0 范围内。

## 开发

```bash
npm install
npm run tauri -- dev
```

## 验证与构建

```bash
npm run typecheck
npm test
npm run build
cd src-tauri
cargo test
cargo clippy -- -D warnings
cd ..
npm run tauri -- build
```

构建结果位于：

- `src-tauri/target/release/bundle/macos/EasyClipboard.app`
- `src-tauri/target/release/bundle/dmg/EasyClipboard_0.1.0_aarch64.dmg`

当前 DMG 使用 ad-hoc 签名，未公证。首次打开可能需要在 Finder 中右键选择“打开”。首次点击“开始记录”会触发系统剪贴板授权；应用还需要在“系统设置 → 隐私与安全性 → 辅助功能”中授权。
