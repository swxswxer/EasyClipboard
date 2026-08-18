import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ComponentProps } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ClipboardPanel } from "./ClipboardPanel";
import type { ClipboardItemSummary } from "../types";

const olderPinned: ClipboardItemSummary = {
  id: "older", kind: "text", title: "较早的固定内容", sourceName: "Notes",
  sourceAppId: "com.apple.Notes", copiedAt: "2026-08-17T08:00:00.000Z", byteSize: 12,
  pinned: true, groupId: null, retained: true, missingFiles: false,
};

const newest: ClipboardItemSummary = {
  ...olderPinned, id: "newest", title: "最新内容", copiedAt: "2026-08-17T09:00:00.000Z",
  pinned: false, retained: false,
};

const groups = [{ id: "common", name: "常用回复", sortOrder: 0, createdAt: "2026-08-17T07:00:00.000Z" }];

function props(overrides: Partial<ComponentProps<typeof ClipboardPanel>> = {}): ComponentProps<typeof ClipboardPanel> {
  return {
    groups, items: [olderPinned, newest], activeGroup: "recent", query: "", selectedId: null, detail: null,
    dialog: null, menu: null, toast: null, nextCursor: null, loading: false, recordingPaused: false,
    permission: { platform: "macos", clipboardAccess: "ready", pasteAutomation: "ready", supportsAppExclusions: true }, searchFocusRequest: 0, onSetActiveGroup: vi.fn(), onSetQuery: vi.fn(),
    onSelect: vi.fn(), onOpenDialog: vi.fn(), onCloseDialog: vi.fn(), onSubmitDialog: vi.fn(),
    onConfirmDelete: vi.fn(), onToggleMoveMenu: vi.fn(), onMoveItem: vi.fn(), onTogglePin: vi.fn(),
    onDeleteItem: vi.fn(), onPaste: vi.fn(), onClosePanel: vi.fn(), onLoadMore: vi.fn(),
    onToggleRecording: vi.fn(), onStartRecording: vi.fn(), onRequestPasteAutomationAccess: vi.fn(),
    onOpenPasteAutomationSettings: vi.fn(), ...overrides,
  };
}

describe("ClipboardPanel", () => {
  beforeEach(() => {
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  });

  it("selects the newest item by default even when an older item is pinned first", async () => {
    const onSelect = vi.fn();
    render(<ClipboardPanel {...props({ onSelect })} />);
    await waitFor(() => expect(onSelect).toHaveBeenCalledWith("newest"));
  });

  it("restores search focus whenever the panel is shown again", async () => {
    const initial = props({ selectedId: "newest" });
    const { rerender } = render(<ClipboardPanel {...initial} />);
    const row = screen.getByRole("option", { name: /最新内容/ });
    row.focus();
    expect(document.activeElement).toBe(row);

    rerender(<ClipboardPanel {...initial} searchFocusRequest={1} />);
    await waitFor(() => expect(document.activeElement).toBe(screen.getByLabelText("搜索剪贴板")));
  });

  it("uses arrow keys and Return from search while keeping the selected row visible", () => {
    const onSelect = vi.fn();
    const onPaste = vi.fn();
    const initial = props({ selectedId: "newest", onSelect, onPaste });
    const { rerender } = render(<ClipboardPanel {...initial} />);
    const search = screen.getByLabelText("搜索剪贴板");
    search.focus();
    vi.mocked(HTMLElement.prototype.scrollIntoView).mockClear();
    expect(fireEvent.keyDown(search, { key: "ArrowDown", code: "ArrowDown" })).toBe(false);
    expect(onSelect).toHaveBeenCalledWith("older");
    rerender(<ClipboardPanel {...initial} selectedId="older" />);
    expect(HTMLElement.prototype.scrollIntoView).toHaveBeenCalled();
    expect(document.activeElement).toBe(search);
    fireEvent.keyDown(search, { key: "Enter", code: "Enter" });
    expect(onPaste).toHaveBeenCalledWith(olderPinned);
  });

  it("moves row focus with arrows and pastes the selected row with Return", () => {
    const onSelect = vi.fn();
    const onPaste = vi.fn();
    const initial = props({ selectedId: "newest", onSelect, onPaste });
    const { rerender } = render(<ClipboardPanel {...initial} />);
    const newestRow = screen.getByRole("option", { name: /最新内容/ });
    newestRow.focus();
    expect(fireEvent.keyDown(newestRow, { key: "ArrowDown", code: "ArrowDown" })).toBe(false);
    expect(onSelect).toHaveBeenCalledWith("older");
    rerender(<ClipboardPanel {...initial} selectedId="older" />);
    const olderRow = screen.getByRole("option", { name: /较早的固定内容/ });
    expect(document.activeElement).toBe(olderRow);
    fireEvent.keyDown(olderRow, { key: "Enter", code: "Enter" });
    expect(onPaste).toHaveBeenCalledWith(olderPinned);
  });

  it("keeps navigation active after clicking a group control", () => {
    const onSelect = vi.fn();
    const onPaste = vi.fn();
    const initial = props({ selectedId: "newest", onSelect, onPaste });
    const { rerender } = render(<ClipboardPanel {...initial} />);
    const groupButton = screen.getByRole("button", { name: "常用回复" });
    fireEvent.pointerDown(groupButton);
    fireEvent.click(groupButton);
    expect(fireEvent.keyDown(groupButton, { key: "ArrowDown", code: "ArrowDown" })).toBe(false);
    expect(onSelect).toHaveBeenCalledWith("older");

    rerender(<ClipboardPanel {...initial} selectedId="older" />);
    const olderRow = screen.getByRole("option", { name: /较早的固定内容/ });
    expect(document.activeElement).toBe(olderRow);
    fireEvent.keyDown(olderRow, { key: "Enter", code: "Enter" });
    expect(onPaste).toHaveBeenCalledWith(olderPinned);
  });

  it("keeps navigation active after clicking an empty panel area", () => {
    const onSelect = vi.fn();
    const onPaste = vi.fn();
    const initial = props({ selectedId: "newest", onSelect, onPaste });
    const { container, rerender } = render(<ClipboardPanel {...initial} />);
    const emptyArea = container.querySelector<HTMLElement>(".preview-pane");
    expect(emptyArea).not.toBeNull();
    fireEvent.pointerDown(emptyArea!);
    expect(fireEvent.keyDown(emptyArea!, { key: "ArrowDown", code: "ArrowDown" })).toBe(false);
    expect(onSelect).toHaveBeenCalledWith("older");

    rerender(<ClipboardPanel {...initial} selectedId="older" />);
    fireEvent.keyDown(document.activeElement ?? emptyArea!, { key: "Enter", code: "Enter" });
    expect(onPaste).toHaveBeenCalledWith(olderPinned);
  });

  it("preserves Return activation for controls reached with Tab", () => {
    const onPaste = vi.fn();
    const onOpenDialog = vi.fn();
    render(<ClipboardPanel {...props({ selectedId: "newest", onPaste, onOpenDialog })} />);
    const createGroup = screen.getByRole("button", { name: /新建分组/ });
    createGroup.focus();
    fireEvent.keyDown(createGroup, { key: "Tab", code: "Tab" });
    fireEvent.keyDown(createGroup, { key: "Enter", code: "Enter" });
    expect(onPaste).not.toHaveBeenCalled();
  });

  it("blocks panel shortcuts without accessibility permission", () => {
    const onSelect = vi.fn();
    const onPaste = vi.fn();
    render(<ClipboardPanel {...props({ permission: { platform: "macos", clipboardAccess: "ready", pasteAutomation: "permission_required", supportsAppExclusions: true }, onSelect, onPaste })} />);
    expect(screen.getByRole("alertdialog")).toHaveTextContent("需要开启辅助功能");
    onSelect.mockClear();
    fireEvent.keyDown(screen.getByLabelText("搜索剪贴板"), { key: "ArrowDown", code: "ArrowDown" });
    fireEvent.keyDown(screen.getByLabelText("搜索剪贴板"), { key: "Enter", code: "Enter" });
    expect(onSelect).not.toHaveBeenCalled();
    expect(onPaste).not.toHaveBeenCalled();
  });

  it("switches an inactive group, renames the active group, and confirms deletion", () => {
    const onSetActiveGroup = vi.fn();
    const onOpenDialog = vi.fn();
    const onConfirmDelete = vi.fn();
    const initial = props({ onSetActiveGroup, onOpenDialog, onConfirmDelete });
    const { rerender } = render(<ClipboardPanel {...initial} />);
    fireEvent.click(screen.getByRole("button", { name: "常用回复" }));
    expect(onSetActiveGroup).toHaveBeenCalledWith("common");
    expect(onOpenDialog).not.toHaveBeenCalled();

    rerender(<ClipboardPanel {...initial} activeGroup="common" />);
    fireEvent.click(screen.getByRole("button", { name: /常用回复，再次点击重命名/ }));
    expect(onOpenDialog).toHaveBeenCalledWith("rename", groups[0]);
    fireEvent.click(screen.getByRole("button", { name: "删除分组 常用回复" }));
    expect(onOpenDialog).toHaveBeenCalledWith("delete", groups[0]);

    rerender(<ClipboardPanel {...initial} activeGroup="common" dialog={{ mode: "delete", groupId: "common", initialName: "常用回复" }} />);
    fireEvent.click(screen.getByRole("button", { name: "删除分组" }));
    expect(onConfirmDelete).toHaveBeenCalledOnce();
  });
});
