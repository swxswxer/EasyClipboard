import type {
  ClipboardItemDetail,
  ClipboardPage,
  ExcludedApp,
  Group,
  DesktopCapabilities,
  PasteOutcome,
  Settings,
} from "./types";

export interface ListItemsOptions {
  query?: string;
  groupId?: string | null;
  cursor?: string | null;
  limit?: number;
}

export interface ClipboardRepository {
  listItems(options: ListItemsOptions): Promise<ClipboardPage>;
  getItem(id: string): Promise<ClipboardItemDetail>;
  pasteItem(id: string): Promise<PasteOutcome>;
  deleteItem(id: string): Promise<void>;
  clearRecent(): Promise<void>;
  deleteAllData(): Promise<void>;
  setPinned(id: string, pinned: boolean): Promise<void>;
  listGroups(): Promise<Group[]>;
  createGroup(name: string): Promise<Group>;
  renameGroup(id: string, name: string): Promise<void>;
  deleteGroup(id: string): Promise<void>;
  moveItem(itemId: string, groupId: string | null): Promise<void>;
  getSettings(): Promise<Settings>;
  updateSettings(patch: Partial<Settings>): Promise<Settings>;
  setGlobalShortcut(shortcut: string): Promise<Settings>;
  getDesktopCapabilities(): Promise<DesktopCapabilities>;
  requestPasteAutomationAccess(): Promise<DesktopCapabilities>;
  openPasteAutomationSettings(): Promise<void>;
  pickExcludedApp(): Promise<ExcludedApp | null>;
  startRecording(): Promise<DesktopCapabilities>;
  hidePanel(): Promise<void>;
  closeSettings(): Promise<void>;
  subscribeChanged(callback: () => void): Promise<() => void>;
  subscribePanelShown(callback: () => void): Promise<() => void>;
}
