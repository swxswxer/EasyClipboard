import { useEffect, useLayoutEffect, useRef, useState, type UIEvent } from "react";
import {
  ArrowDown, ArrowElbowDownLeft, ArrowUp, File, FolderPlus,
  ImageSquare, Infinity as InfinityIcon, MagnifyingGlass, Pause, Play,
  Plus, PushPin, ShieldCheck, TextT, Trash, X,
} from "@phosphor-icons/react";
import type { ClipboardItemDetail, ClipboardItemSummary, Group, DesktopCapabilities } from "../types";

export type PanelDialogState = { mode: "create" | "rename" | "delete" | "delete_item"; groupId?: string; itemId?: string; initialName?: string } | null;
export type MenuState = { type: "move"; itemId: string } | null;

interface ClipboardPanelProps {
  groups: Group[];
  items: ClipboardItemSummary[];
  activeGroup: string;
  query: string;
  selectedId: string | null;
  detail: ClipboardItemDetail | null;
  dialog: PanelDialogState;
  menu: MenuState;
  toast: string | null;
  nextCursor: string | null;
  loading: boolean;
  recordingPaused: boolean;
  permission: DesktopCapabilities | null;
  searchFocusRequest: number;
  onSetActiveGroup: (id: string) => void;
  onSetQuery: (value: string) => void;
  onSelect: (id: string) => void;
  onOpenDialog: (mode: "create" | "rename" | "delete", group?: Group) => void;
  onCloseDialog: () => void;
  onSubmitDialog: (name: string) => void;
  onConfirmDelete: () => void;
  onToggleMoveMenu: (id: string) => void;
  onMoveItem: (id: string, groupId: string | null) => void;
  onTogglePin: (item: ClipboardItemSummary) => void;
  onDeleteItem: (id: string) => void;
  onPaste: (item: ClipboardItemSummary) => void;
  onClosePanel: () => void;
  onLoadMore: () => void;
  onToggleRecording: () => void;
  onStartRecording: () => void;
  onRequestPasteAutomationAccess: () => void;
  onOpenPasteAutomationSettings: () => void;
}

function KeyHint({ label, icon: Icon }: { label?: string; icon?: typeof ArrowUp }) {
  return <span className="key-hint"><span className="keycap">{Icon ? <Icon weight="bold" /> : label}</span></span>;
}

function PanelDialog({ state, onCancel, onSubmit, onDelete }: {
  state: NonNullable<PanelDialogState>;
  onCancel: () => void;
  onSubmit: (name: string) => void;
  onDelete: () => void;
}) {
  const [name, setName] = useState(state.initialName ?? "");
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => { input.current?.focus(); input.current?.select(); }, []);
  if (state.mode === "delete" || state.mode === "delete_item") {
    const deletingItem = state.mode === "delete_item";
    return (
    <div className="dialog-backdrop" role="presentation" onMouseDown={onCancel}>
      <div className="group-dialog delete-dialog" role="alertdialog" aria-modal="true" aria-labelledby="delete-dialog-title" onMouseDown={(event) => event.stopPropagation()}>
        <div className="dialog-title" id="delete-dialog-title">{deletingItem ? "永久删除这条内容？" : `删除分组“${state.initialName}”？`}</div>
        <p>{deletingItem ? "删除后无法恢复，分组或固定状态也不会保留。" : "分组中的内容不会删除，将全部移回“最近”。"}</p>
        <div className="dialog-actions">
          <button className="button secondary" type="button" onClick={onCancel}>取消</button>
          <button className="button danger" type="button" onClick={onDelete}>{deletingItem ? "永久删除" : "删除分组"}</button>
        </div>
      </div>
    </div>
    );
  }
  return (
    <div className="dialog-backdrop" role="presentation" onMouseDown={onCancel}>
      <form className="group-dialog" onMouseDown={(event) => event.stopPropagation()} onSubmit={(event) => {
        event.preventDefault(); if (name.trim()) onSubmit(name.trim());
      }}>
        <div className="dialog-title">{state.mode === "create" ? "新建分组" : "重命名分组"}</div>
        <label>分组名称<input ref={input} value={name} maxLength={20} onChange={(event) => setName(event.target.value)} placeholder="例如：项目资料" /></label>
        <p>分组内的内容会永久保留，直到你主动删除。</p>
        <div className="dialog-actions">
          <button className="button secondary" type="button" onClick={onCancel}>取消</button>
          <button className="button primary" type="submit" disabled={!name.trim()}>{state.mode === "create" ? "创建" : "保存"}</button>
        </div>
      </form>
    </div>
  );
}

function formatTime(value: string) {
  const delta = Date.now() - new Date(value).getTime();
  if (delta < 60_000) return "刚刚";
  if (delta < 3_600_000) return `${Math.floor(delta / 60_000)} 分钟前`;
  if (delta < 86_400_000) return `${Math.floor(delta / 3_600_000)} 小时前`;
  if (delta < 172_800_000) return "昨天";
  return new Date(value).toLocaleDateString("zh-CN", { month: "numeric", day: "numeric" });
}

function Preview({ detail }: { detail: ClipboardItemDetail | null }) {
  if (!detail) return <div className="preview-empty">选择一条内容查看预览</div>;
  return (
    <div className="preview-body">
      {detail.kind === "image" && detail.previewDataUrl && <img className="preview-image" src={detail.previewDataUrl} alt="剪贴板图片预览" />}
      {detail.kind === "files" ? (
        <div className="file-preview">
          {detail.files.map((file) => <span key={file}><File />{file.split(/[\\/]/).pop()}</span>)}
        </div>
      ) : (
        <div className="preview-copy">{detail.content || detail.title}</div>
      )}
      {detail.retained && <span className="retention-note"><InfinityIcon weight="bold" /> {detail.groupId ? "分组内" : "已固定"} · 永久保留</span>}
      {detail.missingFiles && <span className="missing-note">文件已移动或删除，无法粘贴</span>}
    </div>
  );
}

export function ClipboardPanel(props: ClipboardPanelProps) {
  const panelRef = useRef<HTMLElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const itemRefs = useRef(new Map<string, HTMLButtonElement>());
  const focusSelectedRow = useRef(false);
  const tabNavigationActive = useRef(false);
  const newest = props.items.reduce<ClipboardItemSummary | null>((latest, item) => (
    !latest || item.copiedAt > latest.copiedAt ? item : latest
  ), null);
  const selected = props.items.find((item) => item.id === props.selectedId) ?? newest;
  const keyboardState = useRef({ props, selected });
  useLayoutEffect(() => { keyboardState.current = { props, selected }; });
  useEffect(() => { if (selected && selected.id !== props.selectedId) props.onSelect(selected.id); }, [selected?.id, props.selectedId]);

  useLayoutEffect(() => {
    if (props.permission?.pasteAutomation !== "ready" || props.permission.clipboardAccess !== "ready") return;
    tabNavigationActive.current = false;
    const frame = window.requestAnimationFrame(() => searchInputRef.current?.focus({ preventScroll: true }));
    return () => window.cancelAnimationFrame(frame);
  }, [props.searchFocusRequest, props.permission?.pasteAutomation, props.permission?.clipboardAccess]);

  useLayoutEffect(() => {
    if (!selected) return;
    const row = itemRefs.current.get(selected.id);
    row?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
    if (focusSelectedRow.current) {
      focusSelectedRow.current = false;
      row?.focus({ preventScroll: true });
    }
  }, [selected?.id]);

  useEffect(() => {
    const handlePointerDown = () => { tabNavigationActive.current = false; };
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      const { props: currentProps, selected: currentSelected } = keyboardState.current;
      const target = event.target as HTMLElement | null;
      if (event.isComposing) return;
      if (event.key === "Tab") {
        tabNavigationActive.current = true;
        return;
      }
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      if (currentProps.dialog) {
        if (event.key === "Escape") { event.preventDefault(); currentProps.onCloseDialog(); }
        return;
      }
      if (currentProps.menu || currentProps.permission?.pasteAutomation !== "ready" || currentProps.permission.clipboardAccess !== "ready") return;
      const searchFocused = Boolean(target?.matches('input[aria-label="搜索剪贴板"]'));
      const rowFocused = Boolean(target?.closest(".item-row"));
      const tabFocusedControl = tabNavigationActive.current
        && !rowFocused
        && Boolean(target?.closest('button, a[href], select, [role="button"]'));
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        event.stopPropagation();
        if (!currentProps.items.length) return;
        const currentIndex = currentProps.items.findIndex((item) => item.id === currentSelected?.id);
        const direction = event.key === "ArrowDown" ? 1 : -1;
        const nextIndex = currentIndex < 0
          ? (direction > 0 ? 0 : currentProps.items.length - 1)
          : (currentIndex + direction + currentProps.items.length) % currentProps.items.length;
        focusSelectedRow.current = !searchFocused;
        tabNavigationActive.current = false;
        currentProps.onSelect(currentProps.items[nextIndex].id);
      } else if ((event.key === "Enter" || event.code === "NumpadEnter") && currentSelected && !tabFocusedControl) {
        event.preventDefault();
        event.stopPropagation();
        if (!event.repeat) currentProps.onPaste(currentSelected);
      } else if (event.key === "Escape") {
        event.preventDefault();
        currentProps.onClosePanel();
      }
    };
    window.addEventListener("pointerdown", handlePointerDown, true);
    window.addEventListener("keydown", handleKeyDown, true);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown, true);
      window.removeEventListener("keydown", handleKeyDown, true);
    };
  }, []);

  const onListScroll = (event: UIEvent<HTMLDivElement>) => {
    const target = event.currentTarget;
    if (props.nextCursor && !props.loading && target.scrollHeight - target.scrollTop - target.clientHeight < 80) props.onLoadMore();
  };

  return (
    <section ref={panelRef} className="clipboard-panel" aria-label="剪贴板面板">
      <div className="search-row">
        <MagnifyingGlass />
        <input ref={searchInputRef} aria-label="搜索剪贴板" value={props.query} onChange={(event) => props.onSetQuery(event.target.value)} placeholder="搜索剪贴板" />
        {props.query ? (
          <button className="icon-button" aria-label="清空搜索" onClick={() => props.onSetQuery("")}><X /></button>
        ) : (
          <button className={`icon-button ${props.recordingPaused ? "paused" : ""}`} aria-label={props.recordingPaused ? "继续记录" : "暂停记录"} onClick={props.onToggleRecording}>
            {props.recordingPaused ? <Play /> : <Pause />}
          </button>
        )}
      </div>
      <div className="groups-row" aria-label="剪贴板分组">
        <button className={`group-tab ${props.activeGroup === "recent" ? "active" : ""}`} onClick={() => props.onSetActiveGroup("recent")}>最近</button>
        {props.groups.map((group) => (
          <div key={group.id} className={`group-chip ${props.activeGroup === group.id ? "active" : ""}`}>
            <button className={`group-tab ${props.activeGroup === group.id ? "active" : ""}`} onClick={() => {
              if (props.activeGroup === group.id) props.onOpenDialog("rename", group);
              else props.onSetActiveGroup(group.id);
            }} aria-label={props.activeGroup === group.id ? `${group.name}，再次点击重命名` : group.name}>{group.name}</button>
            <button className="group-delete" aria-label={`删除分组 ${group.name}`} onClick={(event) => { event.stopPropagation(); props.onOpenDialog("delete", group); }}><X weight="bold" /></button>
          </div>
        ))}
        <button className="new-group" onClick={() => props.onOpenDialog("create")}><Plus weight="bold" /> 新建分组</button>
      </div>
      <div className="panel-content">
        <div className="items-list" role="listbox" aria-label="剪贴板内容" onScroll={onListScroll}>
          {props.items.length ? props.items.map((item) => {
            const TypeIcon = item.kind === "image" ? ImageSquare : item.kind === "files" ? File : TextT;
            return (
              <button key={item.id} ref={(node) => { if (node) itemRefs.current.set(item.id, node); else itemRefs.current.delete(item.id); }}
                className={`item-row ${selected?.id === item.id ? "selected" : ""}`} role="option" aria-selected={selected?.id === item.id} tabIndex={selected?.id === item.id ? 0 : -1}
                onClick={() => props.onSelect(item.id)} onDoubleClick={() => props.onPaste(item)}>
                <span className="type-icon"><TypeIcon /></span><span className="item-title">{item.title}</span>
                <span className="item-meta">{item.sourceName} · {formatTime(item.copiedAt)}</span>
                {item.retained && <InfinityIcon className="retained-icon" weight="bold" />}
              </button>
            );
          }) : (
            <div className="empty-state"><MagnifyingGlass /><strong>{props.loading ? "正在读取…" : "没有找到内容"}</strong><span>{props.query ? "换个关键词试试" : "复制内容后会出现在这里"}</span></div>
          )}
        </div>
        <div className="preview-pane">
          <Preview detail={props.detail} />
          {selected && (
            <div className="preview-actions">
              <button className={`icon-button action ${selected.pinned ? "active" : ""}`} aria-label={selected.pinned ? "取消固定" : "固定"} onClick={() => props.onTogglePin(selected)}><PushPin weight={selected.pinned ? "fill" : "regular"} /></button>
              <div className="move-action-wrap">
                <button className="icon-button action" aria-label="移入分组" onClick={() => props.onToggleMoveMenu(selected.id)}><FolderPlus /></button>
                {props.menu?.type === "move" && (
                  <div className="popover move-popover"><strong>移入分组</strong>
                    {props.groups.map((group) => <button key={group.id} className={selected.groupId === group.id ? "checked" : ""} onClick={() => props.onMoveItem(selected.id, group.id)}>{group.name}{selected.groupId === group.id && <span>已选择</span>}</button>)}
                    {selected.groupId && <button onClick={() => props.onMoveItem(selected.id, null)}>移出分组</button>}
                  </div>
                )}
              </div>
              <button className="icon-button action danger" aria-label="删除" onClick={() => props.onDeleteItem(selected.id)}><Trash /></button>
              <button className="paste-button" onClick={() => props.onPaste(selected)}>粘贴 <ArrowElbowDownLeft weight="bold" /></button>
            </div>
          )}
        </div>
      </div>
      <div className="shortcut-row"><span><KeyHint icon={ArrowUp} /><KeyHint icon={ArrowDown} /> 选择</span><span><KeyHint label="Enter" /> 粘贴</span><span><KeyHint label="Esc" /> 关闭</span></div>
      {props.toast && <div className="toast" role="status">{props.toast}</div>}
      {props.permission === null && (
        <div className="dialog-backdrop"><div className="permission-card loading-permission" role="status"><div className="permission-icon"><ShieldCheck /></div><h2>正在检查系统权限…</h2></div></div>
      )}
      {props.permission?.pasteAutomation === "permission_required" && (
        <div className="dialog-backdrop"><div className="permission-card" role="alertdialog" aria-modal="true"><div className="permission-icon"><ShieldCheck /></div><h2>需要开启辅助功能</h2><p>EasyClipboard 必须取得辅助功能权限，才能记录历史、切回目标应用并完成粘贴。</p><div className="permission-actions"><button className="button primary" onClick={props.onRequestPasteAutomationAccess}>开启辅助功能</button><button className="button secondary" onClick={props.onOpenPasteAutomationSettings}>打开系统设置</button></div></div></div>
      )}
      {props.permission?.pasteAutomation === "ready" && props.permission.clipboardAccess === "not_requested" && (
        <div className="dialog-backdrop"><div className="permission-card"><div className="permission-icon"><TextT /></div><h2>开始记录剪贴板</h2><p>EasyClipboard 只在本机保存你复制的内容。密码管理器和带敏感标记的内容会被自动忽略。</p><button className="button primary" onClick={props.onStartRecording}>开始记录</button></div></div>
      )}
      {props.dialog && <PanelDialog state={props.dialog} onCancel={props.onCloseDialog} onSubmit={props.onSubmitDialog} onDelete={props.onConfirmDelete} />}
    </section>
  );
}
