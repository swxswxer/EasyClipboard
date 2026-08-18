import { useCallback, useEffect, useRef, useState } from "react";
import { Check, Plus, ShieldCheck, Trash, Warning, X } from "@phosphor-icons/react";
import type { ClipboardRepository } from "../repository";
import type { DesktopCapabilities, Settings } from "../types";
import { RepositoryError } from "../types";

const formatShortcut = (value: string, platform?: DesktopCapabilities["platform"]) => {
  const labels = platform === "windows"
    ? { Command: "Win", Control: "Ctrl", Shift: "Shift", Alt: "Alt" }
    : { Command: "⌘", Control: "⌃", Shift: "⇧", Alt: "⌥" };
  return value.split("+").map((part) => labels[part as keyof typeof labels] ?? part).join(" ");
};

function shortcutFromEvent(event: globalThis.KeyboardEvent) {
  const modifiers: string[] = [];
  if (event.metaKey) modifiers.push("Command");
  if (event.ctrlKey) modifiers.push("Control");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");
  if (["Meta", "Control", "Alt", "Shift"].includes(event.key)) return null;
  if (!modifiers.length || event.key === "Dead" || event.key === "Unidentified") return null;
  const key = event.code.startsWith("Key")
    ? event.code.slice(3)
    : event.code.startsWith("Digit")
      ? event.code.slice(5)
      : event.key === " "
        ? "Space"
        : event.key.length === 1
          ? event.key.toUpperCase()
          : event.key;
  return [...modifiers, key].join("+");
}

function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (checked: boolean) => void; label: string }) {
  return <button type="button" role="switch" aria-label={label} aria-checked={checked} className={`toggle ${checked ? "on" : ""}`} onClick={() => onChange(!checked)}><span /></button>;
}

type DataActionDialogState = {
  mode: "clear_recent" | "delete_all";
  step: 1 | 2;
  busy: boolean;
};

function DataActionDialog({ state, onCancel, onConfirm }: {
  state: DataActionDialogState;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const clearRecent = state.mode === "clear_recent";
  const finalDelete = state.mode === "delete_all" && state.step === 2;
  const title = clearRecent ? "清空普通历史？" : finalDelete ? "最后确认：永久删除？" : "删除全部本地数据？";
  const description = clearRecent
    ? "普通历史会被清空，固定内容和分组中的内容会继续保留。"
    : finalDelete
      ? "此操作无法撤销，剪贴板历史、固定内容、分组和设置都会永久删除。"
      : "这会删除剪贴板历史、固定内容、所有分组和设置，并关闭登录时启动。";
  const confirmLabel = state.busy
    ? "正在处理…"
    : clearRecent
      ? "清空历史"
      : finalDelete
        ? "永久删除"
        : "继续";
  return (
    <div className="dialog-backdrop settings-data-backdrop" role="presentation" onMouseDown={() => { if (!state.busy) onCancel(); }}>
      <div className="group-dialog settings-data-dialog" role="alertdialog" aria-modal="true" aria-labelledby="data-action-title" onMouseDown={(event) => event.stopPropagation()}>
        <div className="data-dialog-icon"><Warning weight="fill" /></div>
        <div className="dialog-title" id="data-action-title">{title}</div>
        <p>{description}</p>
        <div className="dialog-actions">
          <button className="button secondary" type="button" disabled={state.busy} onClick={onCancel}>取消</button>
          <button className={`button ${clearRecent && !finalDelete ? "primary" : "danger"}`} type="button" disabled={state.busy} onClick={onConfirm}>{confirmLabel}</button>
        </div>
      </div>
    </div>
  );
}

export function SettingsPage({ repository }: { repository: ClipboardRepository }) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [permission, setPermission] = useState<DesktopCapabilities | null>(null);
  const [recordingShortcut, setRecordingShortcut] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [dataDialog, setDataDialog] = useState<DataActionDialogState | null>(null);
  const messageTimer = useRef<number | null>(null);

  const notify = useCallback((value: string) => {
    setMessage(value);
    if (messageTimer.current) window.clearTimeout(messageTimer.current);
    messageTimer.current = window.setTimeout(() => setMessage(null), 2400);
  }, []);

  useEffect(() => () => {
    if (messageTimer.current) window.clearTimeout(messageTimer.current);
  }, []);

  useEffect(() => {
    let active = true;
    Promise.all([repository.getSettings(), repository.getDesktopCapabilities()]).then(([nextSettings, nextPermission]) => {
      if (!active) return;
      setSettings(nextSettings); setPermission(nextPermission);
    }).catch(() => { if (active) notify("设置读取失败"); });
    return () => { active = false; };
  }, [notify, repository]);

  const refreshPermission = useCallback(async () => {
    try { setPermission(await repository.getDesktopCapabilities()); }
    catch { notify("权限状态读取失败"); }
  }, [notify, repository]);

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

  const save = async (patch: Partial<Settings>) => {
    try { const next = await repository.updateSettings(patch); setSettings(next); return next; }
    catch { notify("设置保存失败"); return null; }
  };

  const commitShortcut = useCallback(async (value: string) => {
    setRecordingShortcut(false);
    try {
      const next = await repository.setGlobalShortcut(value);
      setSettings(next);
      notify("快捷键已更新");
    } catch (error) {
      notify(error instanceof RepositoryError && error.code === "shortcut_conflict" ? "快捷键已被其他应用占用" : "快捷键设置失败");
    }
  }, [notify, repository]);

  useEffect(() => {
    if (!recordingShortcut) return;
    const captureShortcut = (event: globalThis.KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") {
        setRecordingShortcut(false);
        return;
      }
      if (event.repeat) return;
      const value = shortcutFromEvent(event);
      if (value) void commitShortcut(value);
    };
    window.addEventListener("keydown", captureShortcut, true);
    return () => window.removeEventListener("keydown", captureShortcut, true);
  }, [commitShortcut, recordingShortcut]);

  useEffect(() => {
    if (!dataDialog || dataDialog.busy) return;
    const closeOnEscape = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") setDataDialog(null);
    };
    window.addEventListener("keydown", closeOnEscape, true);
    return () => window.removeEventListener("keydown", closeOnEscape, true);
  }, [dataDialog]);

  const close = () => void repository.closeSettings().catch(() => notify("设置窗口关闭失败"));

  const requestPasteAutomationAccess = async () => {
    try {
      const next = await repository.requestPasteAutomationAccess();
      setPermission(next);
      if (next.pasteAutomation === "permission_required") {
        await repository.openPasteAutomationSettings();
        notify("请在系统设置中开启 EasyClipboard");
      }
    } catch { notify("辅助功能权限请求失败"); }
  };

  const pickExcludedApp = async () => {
    try {
      const app = await repository.pickExcludedApp();
      if (!app) return;
      if (settings?.excludedApps.some((entry) => entry.identifier === app.identifier)) {
        notify("这个应用已在排除列表中");
        return;
      }
      if (settings) await save({ excludedApps: [...settings.excludedApps, app] });
    } catch { notify("应用选择失败"); }
  };

  const confirmDataAction = async () => {
    if (!dataDialog || dataDialog.busy) return;
    if (dataDialog.mode === "delete_all" && dataDialog.step === 1) {
      setDataDialog({ ...dataDialog, step: 2 });
      return;
    }
    const mode = dataDialog.mode;
    setDataDialog({ ...dataDialog, busy: true });
    try {
      if (mode === "clear_recent") {
        await repository.clearRecent();
        notify("普通历史已清空");
      } else {
        await repository.deleteAllData();
        setSettings(await repository.getSettings());
        notify("全部本地数据已删除");
      }
      setDataDialog(null);
    } catch {
      setDataDialog((current) => current ? { ...current, busy: false } : null);
      notify(mode === "clear_recent" ? "普通历史清空失败" : "本地数据删除失败");
    }
  };

  if (!settings) return <main className="settings-stage"><section className="settings-window loading">正在读取设置…</section></main>;

  return (
    <main className="settings-stage">
      <section className="settings-window">
        <header className="settings-header"><div><h1>EasyClipboard 设置</h1><p>所有内容只保存在这台{permission?.platform === "windows" ? " Windows 设备" : " Mac"}</p></div><button className="icon-button" aria-label="关闭设置" onClick={close}><X /></button></header>
        <div className="settings-scroll">
          <section className="settings-card">
            <div className="setting-row"><div><strong>登录时启动</strong><span>开机后在{permission?.platform === "windows" ? "系统托盘" : "菜单栏"}静默运行</span></div><Toggle label="登录时启动" checked={settings.launchAtLogin} onChange={(checked) => void save({ launchAtLogin: checked })} /></div>
            <div className="setting-row"><div><strong>暂停记录</strong><span>暂停后不读取新的剪贴板内容</span></div><Toggle label="暂停记录" checked={settings.recordingPaused} onChange={(checked) => void save({ recordingPaused: checked })} /></div>
          </section>

          <section className="settings-section"><h2>快捷键与粘贴</h2><div className="settings-card">
            <div className="setting-row"><div><strong>打开剪贴板</strong><span>点击右侧后按下新的组合键</span></div>
              <button type="button" className={`shortcut-recorder ${recordingShortcut ? "recording" : ""}`} aria-pressed={recordingShortcut}
                onClick={() => setRecordingShortcut((current) => !current)}>{recordingShortcut ? "请按组合键…（Esc 取消）" : formatShortcut(settings.shortcut, permission?.platform)}</button>
            </div>
            {permission?.platform === "macos" && <>
              <div className="setting-row"><div><strong>辅助功能权限</strong><span>EasyClipboard 使用此权限切回目标应用并完成粘贴</span></div><span className={`permission-status ${permission.pasteAutomation === "ready" ? "granted" : "required"}`}>{permission.pasteAutomation === "ready" ? "已开启" : "必须开启"}</span></div>
              {permission.pasteAutomation === "permission_required" && <div className="permission-row"><Warning /><span>未开启时剪贴板主面板和历史记录将暂停使用。</span><button onClick={() => void requestPasteAutomationAccess()}>开启辅助功能</button></div>}
            </>}
          </div></section>

          <section className="settings-section"><h2>历史保留</h2><div className="settings-card retention-grid">
            <label><span>最多保留</span><select value={settings.maxItems} onChange={(event) => void save({ maxItems: Number(event.target.value) as Settings["maxItems"] })}><option value={100}>100 条</option><option value={500}>500 条</option><option value={1000}>1,000 条</option><option value={5000}>5,000 条</option></select></label>
            <label><span>保留时间</span><select value={settings.retentionDays} onChange={(event) => void save({ retentionDays: Number(event.target.value) as Settings["retentionDays"] })}><option value={7}>7 天</option><option value={30}>30 天</option><option value={90}>90 天</option><option value={0}>不限</option></select></label>
            <p>固定内容和分组中的内容不受自动清理影响。</p>
          </div></section>

          {permission?.supportsAppExclusions && <section className="settings-section"><div className="section-title-row"><h2>排除应用</h2><button className="mini-button" onClick={() => void pickExcludedApp()}><Plus />添加应用</button></div><div className="settings-card excluded-list">
            {settings.excludedApps.map((app) => <div className="excluded-app" key={app.identifier}><span className="app-dot">{app.name.slice(0, 1)}</span><div><strong>{app.name}</strong><span>{app.identifier}</span></div><button className="icon-button danger" aria-label={`移除 ${app.name}`} disabled={app.identifier === "com.easyclipboard.desktop"} onClick={() => void save({ excludedApps: settings.excludedApps.filter((entry) => entry.identifier !== app.identifier) })}><X /></button></div>)}
          </div></section>}

          <section className="settings-section"><h2>本地数据</h2><div className="settings-card">
            <button className="wide-action" onClick={() => setDataDialog({ mode: "clear_recent", step: 1, busy: false })}><Trash />清空普通历史<span>保留固定与分组内容</span></button>
            <button className="wide-action destructive" onClick={() => setDataDialog({ mode: "delete_all", step: 1, busy: false })}><ShieldCheck />删除全部本地数据<span>不可撤销</span></button>
          </div></section>
        </div>
        {message && <div className="settings-toast"><Check />{message}</div>}
        {dataDialog && <DataActionDialog state={dataDialog} onCancel={() => setDataDialog(null)} onConfirm={() => void confirmDataAction()} />}
      </section>
    </main>
  );
}
