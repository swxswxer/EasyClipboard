import { useCallback, useEffect, useRef, useState } from "react";
import type { ClipboardItemDetail, ClipboardItemSummary, ClipboardPage, Group, DesktopCapabilities, Settings } from "./types";
import { RepositoryError } from "./types";
import { ClipboardPanel, type MenuState, type PanelDialogState } from "./components/ClipboardPanel";
import { SettingsPage } from "./components/SettingsPage";
import { repository } from "./repositories";

const errorText = (error: unknown) => {
  if (!(error instanceof RepositoryError)) return "操作失败，请稍后重试";
  return ({
    not_found: "这条内容已经不存在",
    file_missing: "文件已移动或删除，无法粘贴",
    permission_denied: "需要开启辅助功能才能使用 EasyClipboard",
    shortcut_conflict: "快捷键已被其他应用占用",
    content_too_large: "内容超过 MVP 大小限制",
    clipboard_unavailable: "系统剪贴板暂时不可用",
    storage_error: "本地数据操作失败",
  })[error.code];
};

export function App() {
  const windowType = new URLSearchParams(window.location.search).get("window");
  if (windowType === "settings") return <SettingsPage repository={repository} />;
  return <ClipboardApp />;
}

function ClipboardApp() {
  const [groups, setGroups] = useState<Group[]>([]);
  const [page, setPage] = useState<ClipboardPage>({ items: [], nextCursor: null });
  const [detail, setDetail] = useState<ClipboardItemDetail | null>(null);
  const [activeGroup, setActiveGroup] = useState("recent");
  const [query, setQuery] = useState("");
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [dialog, setDialog] = useState<PanelDialogState>(null);
  const [menu, setMenu] = useState<MenuState>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [permission, setPermission] = useState<DesktopCapabilities | null>(null);
  const [searchFocusRequest, setSearchFocusRequest] = useState(0);
  const toastTimer = useRef<number | null>(null);
  const requestToken = useRef(0);
  const nextCursor = useRef<string | null>(null);
  const selectNewestAfterLoad = useRef(false);

  const notify = useCallback((message: string) => {
    setToast(message); if (toastTimer.current) window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 2200);
  }, []);

  useEffect(() => () => {
    if (toastTimer.current) window.clearTimeout(toastTimer.current);
  }, []);

  useEffect(() => { const timer = window.setTimeout(() => setDebouncedQuery(query), 150); return () => window.clearTimeout(timer); }, [query]);

  const load = useCallback(async (append = false) => {
    const token = ++requestToken.current;
    setLoading(true);
    try {
      const next = await repository.listItems({ query: debouncedQuery, groupId: activeGroup === "recent" ? null : activeGroup, cursor: append ? nextCursor.current : null, limit: 100 });
      if (token !== requestToken.current) return;
      nextCursor.current = next.nextCursor;
      setPage((current) => ({ items: append ? [...current.items, ...next.items] : next.items, nextCursor: next.nextCursor }));
      if (!append && selectNewestAfterLoad.current) {
        selectNewestAfterLoad.current = false;
        const newest = next.items.reduce<ClipboardItemSummary | null>((latest, item) => (
          !latest || item.copiedAt > latest.copiedAt ? item : latest
        ), null);
        setSelectedId(newest?.id ?? null);
      }
    } catch (error) { notify(errorText(error)); }
    finally { if (token === requestToken.current) setLoading(false); }
  }, [activeGroup, debouncedQuery, notify]);

  const refresh = useCallback(async () => {
    try {
      const [nextGroups, nextSettings, nextPermission] = await Promise.all([repository.listGroups(), repository.getSettings(), repository.getDesktopCapabilities()]);
      setGroups(nextGroups); setSettings(nextSettings); setPermission(nextPermission);
      await load(false);
    } catch (error) { notify(errorText(error)); }
  }, [load, notify]);

  const refreshPermission = useCallback(async () => {
    try { setPermission(await repository.getDesktopCapabilities()); }
    catch (error) { notify(errorText(error)); }
  }, [notify]);

  const refreshRef = useRef(refresh);
  const loadRef = useRef(load);
  const refreshPermissionRef = useRef(refreshPermission);
  useEffect(() => { refreshRef.current = refresh; }, [refresh]);
  useEffect(() => { loadRef.current = load; }, [load]);
  useEffect(() => { refreshPermissionRef.current = refreshPermission; }, [refreshPermission]);

  useEffect(() => { void load(false); }, [load]);
  useEffect(() => {
    let active = true;
    void Promise.all([repository.listGroups(), repository.getSettings(), repository.getDesktopCapabilities()]).then(([nextGroups, nextSettings, nextPermission]) => {
      if (!active) return;
      setGroups(nextGroups); setSettings(nextSettings); setPermission(nextPermission);
    }).catch((error) => { if (active) notify(errorText(error)); });
    return () => { active = false; };
  }, [notify]);

  useEffect(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void repository.subscribeChanged(() => void refreshRef.current()).then((value) => {
      if (disposed) value(); else cleanup = value;
    }).catch((error) => { if (!disposed) notify(errorText(error)); });
    return () => { disposed = true; cleanup?.(); };
  }, [notify]);

  useEffect(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void repository.subscribePanelShown(() => {
      selectNewestAfterLoad.current = true;
      setSearchFocusRequest((current) => current + 1);
      void refreshPermissionRef.current();
      void loadRef.current(false);
    }).then((value) => {
      if (disposed) value(); else cleanup = value;
    }).catch((error) => { if (!disposed) notify(errorText(error)); });
    return () => { disposed = true; cleanup?.(); };
  }, [notify]);

  useEffect(() => {
    if (permission?.pasteAutomation !== "permission_required") return;
    const timer = window.setInterval(() => void refreshPermission(), 1_000);
    const onFocus = () => void refreshPermission();
    window.addEventListener("focus", onFocus);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  }, [permission?.pasteAutomation, refreshPermission]);

  useEffect(() => {
    if (permission?.pasteAutomation === "permission_required") { setDialog(null); setMenu(null); }
  }, [permission?.pasteAutomation]);

  useEffect(() => {
    if (!selectedId) { setDetail(null); return; }
    let active = true; repository.getItem(selectedId).then((value) => { if (active) setDetail(value); }).catch(() => { if (active) setDetail(null); });
    return () => { active = false; };
  }, [selectedId, page.items]);

  const run = async (action: () => Promise<void>, success?: string) => {
    try { await action(); if (success) notify(success); await refresh(); }
    catch (error) { notify(errorText(error)); }
  };

  const submitDialog = async (name: string) => {
    if (!dialog || (dialog.mode !== "create" && dialog.mode !== "rename")) return;
    try {
      if (dialog.mode === "create") { const group = await repository.createGroup(name); setActiveGroup(group.id); notify(`已创建分组“${name}”`); }
      else if (dialog.groupId) { await repository.renameGroup(dialog.groupId, name); notify("分组已重命名"); }
      setDialog(null); setMenu(null); await refresh();
    } catch (error) { notify(errorText(error)); }
  };

  const confirmDelete = async () => {
    if (dialog?.mode === "delete_item" && dialog.itemId) {
      try {
        await repository.deleteItem(dialog.itemId);
        setDialog(null);
        setMenu(null);
        notify("内容已删除");
      } catch (error) { notify(errorText(error)); }
      return;
    }
    if (dialog?.mode !== "delete" || !dialog.groupId) return;
    const deletingActiveGroup = activeGroup === dialog.groupId;
    try {
      await repository.deleteGroup(dialog.groupId);
      setGroups(await repository.listGroups());
      setDialog(null);
      setMenu(null);
      if (deletingActiveGroup) {
        selectNewestAfterLoad.current = true;
        setActiveGroup("recent");
        setQuery("");
        setSelectedId(null);
      }
      notify("分组已删除，内容已移回最近");
    } catch (error) { notify(errorText(error)); }
  };

  const paste = async (item: ClipboardItemSummary) => {
    try {
      const outcome = await repository.pasteItem(item.id);
      if (outcome.mode === "manual_required") {
        notify({
          elevated_target: "内容已复制，目标应用权限更高，请手动按 Ctrl+V",
          focus_denied: "内容已复制，无法切回目标应用，请手动按 Ctrl+V",
          input_blocked: "内容已复制，系统阻止了按键输入，请手动按 Ctrl+V",
        }[outcome.reason ?? "input_blocked"]);
      }
    } catch (error) {
      if (error instanceof RepositoryError && error.code === "permission_denied") await refreshPermission();
      notify(errorText(error));
    }
  };

  const toggleRecording = async () => {
    if (!settings) return;
    try {
      const next = await repository.updateSettings({ recordingPaused: !settings.recordingPaused });
      setSettings(next);
      notify(next.recordingPaused ? "已暂停记录" : "已继续记录");
    } catch (error) { notify(errorText(error)); }
  };

  const startRecording = async () => {
    try {
      setPermission(await repository.startRecording());
      notify("剪贴板记录已开始");
    } catch (error) { notify(errorText(error)); }
  };

  const requestPasteAutomationAccess = async () => {
    try {
      const next = await repository.requestPasteAutomationAccess();
      setPermission(next);
      if (next.pasteAutomation === "permission_required") await repository.openPasteAutomationSettings();
    } catch (error) { notify(errorText(error)); }
  };

  const openPasteAutomationSettings = async () => {
    try { await repository.openPasteAutomationSettings(); }
    catch (error) { notify(errorText(error)); }
  };

  const closePanel = () => void repository.hidePanel().catch((error) => notify(errorText(error)));
  const panel = (
    <ClipboardPanel groups={groups} items={page.items} activeGroup={activeGroup} query={query} selectedId={selectedId} detail={detail}
      dialog={dialog} menu={menu} toast={toast} nextCursor={page.nextCursor} loading={loading} recordingPaused={settings?.recordingPaused ?? false} permission={permission}
      searchFocusRequest={searchFocusRequest}
      onSetActiveGroup={(id) => { setActiveGroup(id); setQuery(""); setMenu(null); setSelectedId(null); }} onSetQuery={setQuery} onSelect={setSelectedId}
      onOpenDialog={(mode, group) => { setMenu(null); setDialog({ mode, groupId: group?.id, initialName: group?.name }); }} onCloseDialog={() => setDialog(null)} onSubmitDialog={(name) => void submitDialog(name)}
      onConfirmDelete={() => void confirmDelete()}
      onToggleMoveMenu={(id) => setMenu((current) => current?.type === "move" && current.itemId === id ? null : { type: "move", itemId: id })}
      onMoveItem={(id, groupId) => void run(() => repository.moveItem(id, groupId), groupId ? "已移入分组，内容将永久保留" : "已移出分组")}
      onTogglePin={(item) => void run(() => repository.setPinned(item.id, !item.pinned))}
      onDeleteItem={(id) => { setMenu(null); setDialog({ mode: "delete_item", itemId: id }); }}
      onPaste={(item) => void paste(item)} onClosePanel={closePanel} onLoadMore={() => void load(true)}
      onToggleRecording={() => void toggleRecording()}
      onStartRecording={() => void startRecording()}
      onRequestPasteAutomationAccess={() => void requestPasteAutomationAccess()}
      onOpenPasteAutomationSettings={() => void openPasteAutomationSettings()}
    />
  );
  return <main className="app-stage">{panel}</main>;
}
