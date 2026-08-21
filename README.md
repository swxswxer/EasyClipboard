# EasyClipboard 0.1.1

本地优先的 Windows 与 macOS 剪贴板历史 MVP。界面使用同一套 React 深色毛玻璃设计，原生能力由 Rust 平台层分别实现。

## 支持平台

- macOS 13+，Apple Silicon：菜单栏常驻，默认快捷键 `⌘⇧V`，需要辅助功能权限。
- Windows 10/11 x64：系统托盘常驻，默认快捷键 `Ctrl+Shift+V`，不需要权限引导。

Windows 遇到管理员权限目标或系统拒绝焦点/按键注入时，会保留已经写入的剪贴板内容并提示手动按 `Ctrl+V`。Windows 不提供应用排除功能；macOS 保留基于 Bundle ID 的应用排除列表。

## MVP 功能

- 文本、HTML/RTF 表示、PNG/JPEG/TIFF/DIB 图片和多文件历史
- 搜索、固定、删除、自定义分组和分组永久保留
- 全局快捷键切换鼠标所在显示器底部面板
- 自动切回原应用并粘贴，自身回写抑制和内容哈希去重
- 本地 SQLite/FTS5、数量/天数保留规则
- 托盘常驻、暂停记录、登录时启动和单实例运行

账号、云同步、网络请求、自动更新、OCR、macOS Intel 和 Windows ARM64 不在 0.1.1 范围内。

## 目录

```text
src/                         React/TypeScript 页面与仓库接口
src-tauri/src/domain/        跨平台剪贴板规则
src-tauri/src/platform/      macOS 与 Windows 原生实现
src-tauri/src/database.rs    SQLite、FTS 与 schema 迁移
src-tauri/tauri.*.conf.json  公共及平台构建配置
```

## 开发与验证

```bash
npm install
npm run tauri -- dev

npm run typecheck
npm test
npm run build
cd src-tauri
cargo test
cargo clippy -- -D warnings
```

## 安装包

在对应系统本机构建：

```bash
# macOS：.app 与 .dmg
npm run tauri:build

# Windows：当前用户 NSIS -setup.exe
npm run tauri:build -- --bundles nsis
```

输出目录：

- macOS：`src-tauri/target/release/bundle/macos/` 与 `bundle/dmg/`
- Windows：`src-tauri/target/release/bundle/nsis/`

0.1.1 安装包未签名、未公证，仅用于内测。macOS 可能要求在 Finder 中右键“打开”，Windows 可能显示 SmartScreen 提示。
