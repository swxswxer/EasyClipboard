import type { ClipboardRepository, ListItemsOptions } from "../repository";
import type {
  ClipboardItemDetail,
  ClipboardPage,
  ExcludedApp,
  Group,
  DesktopCapabilities,
  PasteOutcome,
  Settings,
} from "../types";
import { RepositoryError } from "../types";

const now = Date.now();
const iso = (minutesAgo: number) => new Date(now - minutesAgo * 60_000).toISOString();

const initialGroups: Group[] = [
  { id: "common", name: "常用回复", sortOrder: 0, createdAt: iso(2_000) },
  { id: "code", name: "代码片段", sortOrder: 1, createdAt: iso(1_900) },
];

const initialItems: ClipboardItemDetail[] = [
  {
    id: "1", kind: "text", title: "收到，我整理后今天发给你。", content: "收到，我整理后今天发给你。",
    sourceName: "微信", sourceAppId: "com.tencent.xinWeChat", copiedAt: iso(0), byteSize: 42,
    groupId: "common", pinned: true, retained: true, missingFiles: false, previewDataUrl: null, files: [],
  },
  {
    id: "2", kind: "text", title: "MVP 只保留基础剪贴板功能", content: "MVP 支持 macOS，本地保存剪贴板历史。",
    sourceName: "VS Code", sourceAppId: "com.microsoft.VSCode", copiedAt: iso(1), byteSize: 58,
    groupId: null, pinned: false, retained: false, missingFiles: false, previewDataUrl: null, files: [],
  },
  {
    id: "3", kind: "files", title: "Tauri 跨平台剪贴板开发计划.md", content: "",
    sourceName: "Finder", sourceAppId: "com.apple.finder", copiedAt: iso(3), byteSize: 18_432,
    groupId: null, pinned: false, retained: false, missingFiles: false, previewDataUrl: null,
    files: ["/Users/example/Documents/Tauri 跨平台剪贴板开发计划.md"],
  },
  {
    id: "4", kind: "image", title: "界面草图.png", content: "PNG 图像 · 1488 × 1058\n1.8 MB",
    sourceName: "截图", sourceAppId: "com.apple.screencaptureui", copiedAt: iso(7), byteSize: 1_800_000,
    groupId: null, pinned: false, retained: false, missingFiles: false,
    previewDataUrl: null, files: [],
  },
  {
    id: "5", kind: "text", title: "https://tauri.app/", content: "https://tauri.app/",
    sourceName: "Safari", sourceAppId: "com.apple.Safari", copiedAt: iso(12), byteSize: 18,
    groupId: "code", pinned: false, retained: true, missingFiles: false, previewDataUrl: null, files: [],
  },
  {
    id: "6", kind: "text", title: "窗口毛玻璃配置", content: "setFrostedGlass({\n  blur: 36,\n  opacity: 0.72\n});",
    sourceName: "VS Code", sourceAppId: "com.microsoft.VSCode", copiedAt: iso(18), byteSize: 68,
    groupId: "code", pinned: false, retained: true, missingFiles: false, previewDataUrl: null, files: [],
  },
  {
    id: "7", kind: "text", title: "好的，感谢同步进度。", content: "好的，感谢同步进度。有变化随时告诉我。",
    sourceName: "企业微信", sourceAppId: "com.tencent.WeWorkMac", copiedAt: iso(1_440), byteSize: 54,
    groupId: "common", pinned: false, retained: true, missingFiles: false, previewDataUrl: null, files: [],
  },
];

const defaultSettings: Settings = {
  shortcut: "Command+Shift+V",
  launchAtLogin: false,
  recordingPaused: false,
  maxItems: 500,
  retentionDays: 30,
  excludedApps: [
    { name: "EasyClipboard", identifier: "com.easyclipboard.desktop" },
    { name: "1Password", identifier: "com.1password.1password" },
    { name: "Bitwarden", identifier: "com.bitwarden.desktop" },
    { name: "KeePassXC", identifier: "org.keepassxc.keepassxc" },
    { name: "Passwords", identifier: "com.apple.Passwords" },
  ],
};

export class TestClipboardRepository implements ClipboardRepository {
  private groups = structuredClone(initialGroups);
  private items = structuredClone(initialItems);
  private settings = structuredClone(defaultSettings);
  private callbacks = new Set<() => void>();
  private settingsCallbacks = new Set<() => void>();

  reset() {
    this.groups = structuredClone(initialGroups);
    this.items = structuredClone(initialItems);
    this.settings = structuredClone(defaultSettings);
    this.callbacks.clear();
    this.settingsCallbacks.clear();
  }

  async listItems({ query = "", groupId = null, cursor = null, limit = 100 }: ListItemsOptions): Promise<ClipboardPage> {
    const offset = Number(cursor ?? 0);
    const needle = query.trim().toLocaleLowerCase();
    const filtered = this.items
      .filter((item) => !groupId || item.groupId === groupId)
      .filter((item) => !needle || `${item.title} ${item.content} ${item.files.join(" ")} ${item.sourceName}`.toLocaleLowerCase().includes(needle))
      .sort((a, b) => b.copiedAt.localeCompare(a.copiedAt));
    const items = filtered.slice(offset, offset + limit).map(({ content: _content, previewDataUrl: _preview, files: _files, ...item }) => item);
    return { items, nextCursor: offset + limit < filtered.length ? String(offset + limit) : null };
  }

  async getItem(id: string) {
    const item = this.items.find((entry) => entry.id === id);
    if (!item) throw new RepositoryError("not_found");
    return structuredClone(item);
  }

  async pasteItem(id: string): Promise<PasteOutcome> {
    const newestTime = this.items.reduce((latest, item) => Math.max(latest, new Date(item.copiedAt).getTime()), 0);
    const copiedAt = new Date(Math.max(Date.now(), newestTime + 1)).toISOString();
    this.items = this.items.map((item) => item.id === id ? { ...item, copiedAt } : item);
    this.changed();
    return { mode: "pasted" };
  }
  async deleteItem(id: string) { this.items = this.items.filter((item) => item.id !== id); this.changed(); }
  async clearRecent() { this.items = this.items.filter((item) => item.groupId || item.pinned); this.changed(); }
  async deleteAllData() {
    this.items = [];
    this.groups = [];
    this.settings = structuredClone(defaultSettings);
    this.changed();
    this.settingsChanged();
  }
  async setPinned(id: string, pinned: boolean) { this.mutate(id, (item) => ({ ...item, pinned, retained: pinned || Boolean(item.groupId) })); }
  async listGroups() { return structuredClone(this.groups); }
  async createGroup(name: string) {
    const group = { id: `group-${Date.now()}`, name, sortOrder: this.groups.length, createdAt: new Date().toISOString() };
    this.groups.push(group); this.changed(); return structuredClone(group);
  }
  async renameGroup(id: string, name: string) { this.groups = this.groups.map((group) => group.id === id ? { ...group, name } : group); this.changed(); }
  async deleteGroup(id: string) {
    this.groups = this.groups.filter((group) => group.id !== id);
    this.items = this.items.map((item) => item.groupId === id ? { ...item, groupId: null, retained: item.pinned } : item);
    this.changed();
  }
  async moveItem(itemId: string, groupId: string | null) { this.mutate(itemId, (item) => ({ ...item, groupId, retained: item.pinned || Boolean(groupId) })); }
  async getSettings() { return structuredClone(this.settings); }
  async updateSettings(patch: Partial<Settings>) {
    this.settings = { ...this.settings, ...patch };
    this.settingsChanged();
    return this.getSettings();
  }
  async setGlobalShortcut(shortcut: string) {
    this.settings.shortcut = shortcut;
    this.settingsChanged();
    return this.getSettings();
  }
  async getDesktopCapabilities(): Promise<DesktopCapabilities> { return { platform: "macos", clipboardAccess: "ready", pasteAutomation: "ready", supportsAppExclusions: true }; }
  async requestPasteAutomationAccess(): Promise<DesktopCapabilities> { return { platform: "macos", clipboardAccess: "ready", pasteAutomation: "ready", supportsAppExclusions: true }; }
  async openPasteAutomationSettings() {}
  async pickExcludedApp(): Promise<ExcludedApp | null> { return { name: "示例应用", identifier: "com.example.app" }; }
  async startRecording(): Promise<DesktopCapabilities> { return { platform: "macos", clipboardAccess: "ready", pasteAutomation: "ready", supportsAppExclusions: true }; }
  async hidePanel() {}
  async closeSettings() { window.location.search = ""; }
  async subscribeChanged(callback: () => void) { this.callbacks.add(callback); return () => this.callbacks.delete(callback); }
  async subscribeSettingsChanged(callback: () => void) { this.settingsCallbacks.add(callback); return () => this.settingsCallbacks.delete(callback); }
  async subscribePanelShown() { return () => {}; }

  private mutate(id: string, update: (item: ClipboardItemDetail) => ClipboardItemDetail) {
    this.items = this.items.map((item) => item.id === id ? update(item) : item);
    this.changed();
  }
  private changed() { for (const callback of this.callbacks) callback(); }
  private settingsChanged() { for (const callback of this.settingsCallbacks) callback(); }
}
