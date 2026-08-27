# EasyClipboard 项目结构说明

EasyClipboard 当前采用“React 前端 + Tauri/Rust 公共业务层 + macOS/Windows 平台适配层”的跨平台结构。

## 1. 总体结构

```text
EasyClipboard/
├── src/                         React 前端
│   ├── components/              主面板、设置页
│   ├── repositories/            Tauri IPC 数据适配
│   ├── test/                    前端测试仓库与测试环境
│   ├── App.tsx                  前端状态与业务编排
│   ├── repository.ts            数据仓库接口
│   ├── types.ts                 前端 DTO 类型
│   └── styles.css               统一毛玻璃样式
│
├── src-tauri/                   Tauri/Rust 桌面端
│   ├── src/
│   │   ├── domain/              跨平台剪贴板业务规则
│   │   ├── platform/            macOS/Windows 原生实现
│   │   ├── commands.rs          前端 IPC 命令
│   │   ├── database.rs          SQLite 与文件存储
│   │   ├── app_state.rs         运行时状态
│   │   ├── windowing.rs         窗口管理
│   │   ├── models.rs            Rust DTO 与设置模型
│   │   ├── error.rs             错误码
│   │   ├── lib.rs               Tauri 应用入口与后台循环
│   │   └── main.rs              可执行程序入口
│   ├── capabilities/            Tauri IPC 权限
│   ├── icons/                   跨平台应用图标
│   ├── tests/                   原生集成测试
│   └── tauri.*.conf.json        公共及平台配置
│
├── .github/workflows/           标签发布流水线
├── package.json                 前端依赖与命令
├── vite.config.mjs              Vite/Vitest 配置
├── README.md                    使用和构建说明
└── Tauri跨平台剪贴板开发计划.md  产品及开发计划
```

## 2. React 前端

### 2.1 应用入口

#### [src/main.tsx](src/main.tsx)

- 挂载 React。
- 加载全局样式。
- 启动顶层 `App`。

#### [src/App.tsx](src/App.tsx)

这是前端主要的业务编排层，负责：

- 区分主面板和设置窗口。
- 加载剪贴板记录、分组、设置和权限状态。
- 管理搜索、选中条目、当前分组、弹窗、悬浮菜单和提示。
- 处理搜索防抖和游标分页。
- 监听 Rust 发出的剪贴板变化、设置变化、面板显示和隐藏事件。
- 面板关闭时恢复初始状态。
- 调用仓库完成粘贴、固定、删除、改标题和分组操作。
- 处理 Windows 手动粘贴降级提示和 macOS 权限状态。

### 2.2 页面组件

#### [src/components/ClipboardPanel.tsx](src/components/ClipboardPanel.tsx)

主剪贴板窗口，负责：

- 搜索框。
- “最近”和自定义分组标签。
- 左侧记录列表。
- 右侧内容预览。
- 固定、修改标题、删除和移入分组。
- 新建、重命名、删除分组对话框。
- 键盘上下选择、Return 粘贴、Esc 关闭。
- 上一组和下一组快捷键。
- 移组悬浮菜单的点击外部关闭。
- macOS 辅助功能权限引导。

#### [src/components/SettingsPage.tsx](src/components/SettingsPage.tsx)

设置窗口，负责：

- 登录时启动。
- 暂停记录。
- 全局唤起快捷键。
- 上一组和下一组快捷键。
- 历史数量与保留时间。
- macOS 辅助功能权限。
- macOS 应用排除列表。
- 清空普通历史。
- 删除全部本地数据。

#### [src/styles.css](src/styles.css)

Windows 和 macOS 共用的视觉样式：

- 深色毛玻璃。
- 窗口、列表、预览和设置页布局。
- 弹窗、悬浮菜单、按钮和滚动条。
- 选中状态及键盘提示。
- 跨平台统一，不维护两套视觉主题。

### 2.3 前端数据接口

#### [src/types.ts](src/types.ts)

定义前端 DTO：

- 剪贴板摘要和详情。
- 文本、图片、文件类型。
- 分组。
- 设置。
- 平台能力。
- 粘贴结果。
- 稳定错误码。

#### [src/repository.ts](src/repository.ts)

定义 `ClipboardRepository` 接口。页面只依赖该接口，不直接调用 Tauri。

```text
页面 → ClipboardRepository → Tauri IPC → Rust Command
```

#### [src/repositories/tauri.ts](src/repositories/tauri.ts)

正式运行时的数据适配器：

- 使用 `invoke` 调用 Rust Command。
- 使用 `listen` 订阅 Rust 事件。
- 将 Rust 错误转换成前端 `RepositoryError`。
- 合并局部设置更新。

#### [src/repositories/index.ts](src/repositories/index.ts)

创建正式的 `TauriClipboardRepository` 实例。生产代码没有浏览器 Mock 分支。

## 3. Rust/Tauri 公共层

### 3.1 应用入口和后台任务

#### [src-tauri/src/lib.rs](src-tauri/src/lib.rs)

桌面应用的核心启动文件：

- 注册 Tauri 插件。
- 注册全局快捷键。
- 注册前端可调用的 Commands。
- 初始化数据库和运行状态。
- 设置 macOS 菜单栏模式并隐藏 Dock 图标。
- 创建托盘菜单。
- 启动剪贴板监控任务。
- 根据系统调用不同平台的剪贴板实现。
- 运行每日历史清理。
- 失焦时隐藏主面板。

#### [src-tauri/src/main.rs](src-tauri/src/main.rs)

可执行程序入口，只负责调用库中的 `run()`。

### 3.2 IPC 命令层

#### [src-tauri/src/commands.rs](src-tauri/src/commands.rs)

连接 React 与 Rust 业务层，提供：

- 查询记录与详情。
- 粘贴、删除、固定、修改标题。
- 分组 CRUD 和移动项目。
- 设置读取与保存。
- 全局快捷键更新。
- 平台能力与权限。
- 窗口关闭。
- 数据变化事件发送。

该层负责流程编排，不负责 Win32 或 AppKit 的具体实现。

### 3.3 运行时状态

#### [src-tauri/src/app_state.rs](src-tauri/src/app_state.rs)

保存不能直接落库的临时状态：

- 剪贴板是否开始记录。
- 最近一次剪贴板变化标识。
- 自身写回抑制时间。
- 预期内容哈希。
- 粘贴目标应用。
- 最近一次剪贴板变化时间。

主要用于防止应用自己的剪贴板写回被再次保存：

```text
选择历史记录粘贴
→ EasyClipboard 写回系统剪贴板
→ 系统通知剪贴板变化
→ EasyClipboard 识别为自身写回并跳过
```

### 3.4 公共数据模型

#### [src-tauri/src/models.rs](src-tauri/src/models.rs)

Rust 侧 DTO 和持久化设置：

- `ClipboardItemSummary`
- `ClipboardItemDetail`
- `Group`
- `Settings`
- `DesktopCapabilities`
- `CapturedClipboard`

同时负责旧设置 JSON 的兼容。例如旧版本不存在切组快捷键时，会自动补齐平台默认值。

#### [src-tauri/src/error.rs](src-tauri/src/error.rs)

统一错误类型及稳定错误码，例如：

- `not_found`
- `file_missing`
- `permission_denied`
- `clipboard_busy`
- `clipboard_write_failed`
- `storage_error`

## 4. 公共剪贴板业务层

#### [src-tauri/src/domain/clipboard.rs](src-tauri/src/domain/clipboard.rs)

与操作系统无关的剪贴板规则：

- 文本、图片、文件数量和大小限制。
- 文本标题生成。
- 文件标题生成。
- SHA-256 内容哈希。
- 图片统一转换为 PNG。
- 单个图片文件识别为图片内容。
- 图片尺寸解析。
- 内容去重所需的统一哈希规则。

该层不能直接调用 `NSPasteboard` 或 Win32 API。

## 5. 平台抽象层

#### [src-tauri/src/platform/mod.rs](src-tauri/src/platform/mod.rs)

通过条件编译选择平台：

```rust
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "windows")]
pub use windows::*;
```

上层只调用统一接口，不需要到处判断当前操作系统。

#### [src-tauri/src/platform/types.rs](src-tauri/src/platform/types.rs)

定义跨平台原生契约：

- 当前粘贴目标应用。
- 粘贴结果。
- Windows 手动粘贴原因。

## 6. macOS 原生层

#### [src-tauri/src/platform/macos/clipboard.rs](src-tauri/src/platform/macos/clipboard.rs)

基于 `NSPasteboard`：

- 读取 `changeCount`。
- 读取文本、HTML、RTF、图片和文件。
- 写回系统剪贴板。
- 识别 transient/concealed 敏感标记。
- 生成写回后的变化标识和内容哈希。

#### [src-tauri/src/platform/macos/desktop.rs](src-tauri/src/platform/macos/desktop.rs)

负责 macOS 桌面交互：

- 获取当前前台应用。
- 激活粘贴目标应用。
- 模拟 `⌘V`。
- 让主窗口进入当前全屏应用所在 Space。
- 选择需要加入排除列表的 `.app`。

#### [src-tauri/src/platform/macos/permissions.rs](src-tauri/src/platform/macos/permissions.rs)

负责辅助功能权限：

- 检查授权状态。
- 请求辅助功能权限。
- 打开系统隐私设置。

#### [src-tauri/src/platform/macos/mod.rs](src-tauri/src/platform/macos/mod.rs)

汇总 macOS 实现，并声明：

- 默认快捷键 `Command+Shift+V`。
- 默认切组快捷键。
- HUD 毛玻璃效果。
- 应用排除逻辑。
- macOS 不使用系统剪贴板消息窗口，后台通过变化计数轮询。

## 7. Windows 原生层

#### [src-tauri/src/platform/windows/clipboard.rs](src-tauri/src/platform/windows/clipboard.rs)

使用 Win32 Clipboard API：

- 创建隐藏消息窗口。
- 注册 `AddClipboardFormatListener`。
- 接收 `WM_CLIPBOARDUPDATE`。
- 读取和写入 Unicode 文本、HTML、RTF、PNG、DIB/DIBV5 和文件列表。
- 处理剪贴板被其他程序占用的重试。
- 识别 Windows 敏感或禁止监控格式。
- 将图片文件读取并保存为图片 Blob。

#### [src-tauri/src/platform/windows/desktop.rs](src-tauri/src/platform/windows/desktop.rs)

负责 Windows 桌面操作：

- 获取前台 HWND、PID 和进程名。
- 恢复粘贴目标窗口。
- 使用 `SendInput` 发送 `Ctrl+V`。
- 区分焦点失败、权限等级目标和输入被阻止等情况。

#### [src-tauri/src/platform/windows/permissions.rs](src-tauri/src/platform/windows/permissions.rs)

Windows 没有 macOS 式辅助功能授权门槛，因此这里提供统一接口的“已就绪”实现。

#### [src-tauri/src/platform/windows/mod.rs](src-tauri/src/platform/windows/mod.rs)

汇总 Windows 能力：

- 默认快捷键 `Control+Shift+V`。
- 默认切组快捷键。
- Acrylic 毛玻璃。
- 启动后自动记录。
- 清空应用排除设置。
- Windows 不提供应用排除选择器。

## 8. 数据库与本地文件

#### [src-tauri/src/database.rs](src-tauri/src/database.rs)

负责 SQLite 和 Blob 文件：

- SQLite WAL 模式。
- Schema v1 → v2 → v3 迁移。
- 剪贴板记录、表示、分组和设置。
- FTS5 全文搜索。
- 游标分页。
- 内容哈希去重。
- 相同内容重新复制时更新时间并移动到顶部。
- 固定和分组内容永久保留。
- 删除分组时内容移回“最近”。
- 图片原图与缩略图文件。
- 孤立 Blob 清理。
- 数量和时间保留规则。
- 标题修改和搜索索引同步。

数据存放在系统分配给 `com.easyclipboard.desktop` 的应用数据目录中。

## 9. 窗口管理

#### [src-tauri/src/windowing.rs](src-tauri/src/windowing.rs)

负责所有窗口生命周期：

- 快捷键切换主面板显示和隐藏。
- 记录打开面板前的目标应用。
- 根据鼠标所在显示器定位窗口。
- 在屏幕底部计算窗口尺寸和位置。
- 发送 `clipboard://shown`、`clipboard://hidden`。
- 创建、显示和关闭设置窗口。
- Windows 不显示任务栏图标。
- macOS 支持在当前全屏 Space 显示。

## 10. 配置和安装包

#### [src-tauri/tauri.conf.json](src-tauri/tauri.conf.json)

公共配置：

- 产品名和版本。
- Bundle Identifier。
- 前端构建命令。
- CSP。
- 基础打包开关。

#### [src-tauri/tauri.macos.conf.json](src-tauri/tauri.macos.conf.json)

macOS 专属配置：

- 隐藏并预创建主窗口。
- HUD 毛玻璃。
- 全工作区显示。
- Apple Silicon `.app` 和 `.dmg`。
- macOS 13 最低版本。
- DMG 拖入 Applications 布局。

#### [src-tauri/tauri.windows.conf.json](src-tauri/tauri.windows.conf.json)

Windows 专属配置：

- Acrylic 效果。
- 不进入任务栏。
- 不使用原生矩形阴影。
- NSIS 当前用户安装。
- WebView2 Bootstrapper。
- 中英文安装器。

#### [src-tauri/capabilities/default.json](src-tauri/capabilities/default.json)

Tauri 最小权限配置，只允许两个窗口使用必要的 Core 和事件监听能力，没有开放 HTTP、Shell 或任意文件读取。

#### [src-tauri/icons/](src-tauri/icons/)

保存 Tauri 打包所需的 PNG、ICNS 和 ICO 应用图标。

## 11. 测试

#### [src/App.test.tsx](src/App.test.tsx)

覆盖前端业务流程，例如搜索、粘贴、权限、状态重置、改标题和分组移动。

#### [src/components/ClipboardPanel.test.tsx](src/components/ClipboardPanel.test.tsx)

覆盖主面板键盘导航、分组、弹窗、悬浮菜单和快捷键交互。

#### [src/components/SettingsPage.test.tsx](src/components/SettingsPage.test.tsx)

覆盖设置保存、快捷键录制、系统差异和数据删除确认。

#### [src/test/TestClipboardRepository.ts](src/test/TestClipboardRepository.ts)

只供测试使用的内存仓库，不进入正式产品数据链路。

#### [src-tauri/tests/pasteboard_integration.rs](src-tauri/tests/pasteboard_integration.rs)

macOS 独立 Pasteboard 集成测试。

Rust 各模块内部还包含数据库迁移、去重、哈希、图片和粘贴条件单元测试。

## 12. 构建与开发文件

#### [package.json](package.json)

定义 React、Tauri、TypeScript、Vite 和 Vitest 依赖，以及开发、测试和构建命令。

#### [package-lock.json](package-lock.json)

锁定 npm 依赖版本，保证本地和流水线安装一致。

#### [src-tauri/Cargo.toml](src-tauri/Cargo.toml)

定义 Rust 依赖，并通过 target 区块区分 macOS 和 Windows 原生依赖。

#### [src-tauri/Cargo.lock](src-tauri/Cargo.lock)

锁定 Rust 依赖版本。

#### [vite.config.mjs](vite.config.mjs)

配置：

- React 插件。
- 前端输出目录 `dist/client`。
- Vitest 的 jsdom 环境。
- 测试文件路径和初始化脚本。

#### [tsconfig.json](tsconfig.json)

配置 TypeScript 编译和类型检查。

#### [index.html](index.html)

Tauri WebView 加载的前端 HTML 入口。

#### [src-tauri/build.rs](src-tauri/build.rs)

执行 Tauri 的 Rust 构建期代码生成。

#### [src-tauri/Info.plist](src-tauri/Info.plist)

补充 macOS 应用元数据和菜单栏应用相关配置。

## 13. 发布流水线

#### [.github/workflows/release.yml](.github/workflows/release.yml)

推送 `v*` 标签后：

1. macOS Runner 构建 Apple Silicon DMG。
2. Windows Runner 构建 x64 NSIS EXE。
3. 下载两个平台的构建产物。
4. 创建 GitHub Release。
5. 将 DMG 和 EXE 上传为 Release 附件。

## 14. 文档和仓库配置

#### [README.md](README.md)

面向开发者和内测用户的项目介绍、平台范围、构建及安装说明。

#### [Tauri跨平台剪贴板开发计划.md](Tauri跨平台剪贴板开发计划.md)

产品需求、MVP 范围、实现方案和测试计划。

#### [AGENTS.md](AGENTS.md)

项目开发约束和已确认的产品决策。

#### [.gitignore](.gitignore)

排除依赖、构建产物、日志、系统文件和 IDE 本地配置。

## 15. 整体调用关系

```text
React 页面
  ↓ ClipboardRepository
Tauri invoke / event
  ↓
commands.rs
  ├── database.rs
  ├── windowing.rs
  └── platform 统一接口
          ├── macOS：NSPasteboard + Accessibility
          └── Windows：Win32 Clipboard + SendInput
```

## 16. 剪贴板记录流程

```text
用户复制内容
  ↓
macOS changeCount 轮询 / Windows WM_CLIPBOARDUPDATE
  ↓
平台层读取原生剪贴板格式
  ↓
domain 层进行大小限制、图片标准化和哈希计算
  ↓
app_state 判断是否为应用自身写回
  ↓
database 按内容哈希新增或更新记录
  ↓
发送 clipboard://changed
  ↓
React 重新加载当前列表
```

## 17. 历史项目粘贴流程

```text
用户选择历史记录
  ↓
React 调用 paste_item
  ↓
commands 检查权限、目标应用和文件有效性
  ↓
database 读取正文及原始表示
  ↓
平台层写回系统剪贴板
  ↓
记录自身写回抑制信息
  ↓
隐藏主面板并恢复目标应用
  ↓
macOS 发送 ⌘V / Windows 发送 Ctrl+V
  ↓
更新时间并将该记录移动到顶部
```
