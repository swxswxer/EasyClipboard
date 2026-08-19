import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ClipboardRepository, ListItemsOptions } from "../repository";
import type {
  ClipboardItemDetail, ClipboardPage, ExcludedApp, Group, DesktopCapabilities, PasteOutcome, Settings,
} from "../types";
import { RepositoryError, type RepositoryErrorShape } from "../types";

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    if (typeof error === "object" && error && "code" in error) {
      throw new RepositoryError(error as RepositoryErrorShape);
    }
    throw new RepositoryError("storage_error", String(error));
  }
}

export class TauriClipboardRepository implements ClipboardRepository {
  listItems(options: ListItemsOptions): Promise<ClipboardPage> {
    return call("list_items", {
      query: options.query ?? "", groupId: options.groupId ?? null,
      cursor: options.cursor ?? null, limit: options.limit ?? 100,
    });
  }
  getItem(id: string): Promise<ClipboardItemDetail> { return call("get_item", { id }); }
  pasteItem(id: string): Promise<PasteOutcome> { return call("paste_item", { id }); }
  deleteItem(id: string): Promise<void> { return call("delete_item", { id }); }
  clearRecent(): Promise<void> { return call("clear_recent"); }
  deleteAllData(): Promise<void> { return call("delete_all_data"); }
  setPinned(id: string, pinned: boolean): Promise<void> { return call("set_pinned", { id, pinned }); }
  listGroups(): Promise<Group[]> { return call("list_groups"); }
  createGroup(name: string): Promise<Group> { return call("create_group", { name }); }
  renameGroup(id: string, name: string): Promise<void> { return call("rename_group", { id, name }); }
  deleteGroup(id: string): Promise<void> { return call("delete_group", { id }); }
  moveItem(itemId: string, groupId: string | null): Promise<void> { return call("move_item", { itemId, groupId }); }
  getSettings(): Promise<Settings> { return call("get_settings"); }
  async updateSettings(patch: Partial<Settings>): Promise<Settings> {
    const current = await this.getSettings();
    return call("update_settings", { settings: { ...current, ...patch } });
  }
  setGlobalShortcut(shortcut: string): Promise<Settings> { return call("set_global_shortcut", { shortcut }); }
  getDesktopCapabilities(): Promise<DesktopCapabilities> { return call("get_desktop_capabilities"); }
  requestPasteAutomationAccess(): Promise<DesktopCapabilities> { return call("request_paste_automation_access"); }
  openPasteAutomationSettings(): Promise<void> { return call("open_paste_automation_settings"); }
  pickExcludedApp(): Promise<ExcludedApp | null> { return call("pick_excluded_app"); }
  startRecording(): Promise<DesktopCapabilities> { return call("start_recording"); }
  hidePanel(): Promise<void> { return call("hide_panel"); }
  closeSettings(): Promise<void> { return call("close_settings"); }
  subscribeChanged(callback: () => void): Promise<() => void> {
    return listen("clipboard://changed", callback);
  }
  subscribeSettingsChanged(callback: () => void): Promise<() => void> {
    return listen("settings://changed", callback);
  }
  subscribePanelShown(callback: () => void): Promise<() => void> {
    return listen("clipboard://shown", callback);
  }
}
