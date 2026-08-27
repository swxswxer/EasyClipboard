import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./repositories", async () => {
  const actual = await vi.importActual<typeof import("./test/TestClipboardRepository")>("./test/TestClipboardRepository");
  return { repository: new actual.TestClipboardRepository() };
});

import { App } from "./App";
import { repository } from "./repositories";

describe("App", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
    window.history.replaceState({}, "", "/");
    (repository as typeof repository & { reset: () => void }).reset();
  });

  it("debounces search input by 150 ms", async () => {
    const listItems = vi.spyOn(repository, "listItems");
    vi.useFakeTimers();
    render(<App />);
    await act(async () => { await Promise.resolve(); });
    const initialCalls = listItems.mock.calls.length;
    fireEvent.change(screen.getByLabelText("搜索剪贴板"), { target: { value: "MVP" } });
    act(() => { vi.advanceTimersByTime(149); });
    expect(listItems.mock.calls.length).toBe(initialCalls);
    await act(async () => { vi.advanceTimersByTime(1); await Promise.resolve(); });
    expect(listItems.mock.calls.some(([options]) => options.query === "MVP")).toBe(true);
  });

  it("selects the first item after clearing a search", async () => {
    render(<App />);
    const search = await screen.findByLabelText("搜索剪贴板");

    fireEvent.change(search, { target: { value: "MVP" } });
    const matched = await screen.findByRole("option", { name: /MVP 只保留基础剪贴板功能/ });
    await waitFor(() => expect(matched).toHaveAttribute("aria-selected", "true"));

    fireEvent.change(search, { target: { value: "" } });
    const newest = await screen.findByRole("option", { name: /收到，我整理后今天发给你/ });
    await waitFor(() => expect(newest).toHaveAttribute("aria-selected", "true"));
    expect(matched).toHaveAttribute("aria-selected", "false");
  });

  it("pastes a selected item without a copy-only fallback", async () => {
    const pasteItem = vi.spyOn(repository, "pasteItem");
    render(<App />);
    await waitFor(() => expect(screen.getByText("MVP 只保留基础剪贴板功能")).toBeInTheDocument());
    fireEvent.doubleClick(screen.getByText("MVP 只保留基础剪贴板功能"));
    await waitFor(() => expect(pasteItem).toHaveBeenCalledWith("2"));
    expect(screen.queryByText(/已复制/)).not.toBeInTheDocument();
  });

  it("explains the Windows manual-paste fallback after clipboard write succeeds", async () => {
    vi.spyOn(repository, "pasteItem").mockResolvedValue({ mode: "manual_required", reason: "elevated_target" });
    render(<App />);
    await waitFor(() => expect(screen.getByText("MVP 只保留基础剪贴板功能")).toBeInTheDocument());
    fireEvent.doubleClick(screen.getByText("MVP 只保留基础剪贴板功能"));
    expect(await screen.findByText(/目标应用权限更高.*Ctrl\+V/)).toBeInTheDocument();
  });

  it("pastes the focused clipboard row with the macOS Return key", async () => {
    const pasteItem = vi.spyOn(repository, "pasteItem");
    render(<App />);
    const row = await screen.findByRole("option", { name: /MVP 只保留基础剪贴板功能/ });
    fireEvent.click(row);
    row.focus();
    fireEvent.keyDown(row, { key: "Enter", code: "Enter" });
    await waitFor(() => expect(pasteItem).toHaveBeenCalledWith("2"));
  });

  it("locks the main panel until accessibility permission is granted", async () => {
    vi.spyOn(repository, "getDesktopCapabilities").mockResolvedValue({ platform: "macos", clipboardAccess: "ready", pasteAutomation: "permission_required", supportsAppExclusions: true });
    const requestPasteAutomationAccess = vi.spyOn(repository, "requestPasteAutomationAccess");
    render(<App />);
    expect(await screen.findByRole("alertdialog")).toHaveTextContent("需要开启辅助功能");
    fireEvent.click(screen.getByRole("button", { name: "开启辅助功能" }));
    await waitFor(() => expect(requestPasteAutomationAccess).toHaveBeenCalledOnce());
    await waitFor(() => expect(screen.queryByText("需要开启辅助功能")).not.toBeInTheDocument());
  });

  it("does not show a permission gate on Windows", async () => {
    vi.spyOn(repository, "getDesktopCapabilities").mockResolvedValue({
      platform: "windows", clipboardAccess: "ready", pasteAutomation: "ready", supportsAppExclusions: false,
    });
    render(<App />);
    expect(await screen.findByLabelText("搜索剪贴板")).toBeEnabled();
    expect(screen.queryByText("需要开启辅助功能")).not.toBeInTheDocument();
  });

  it("uses an in-app confirmation before deleting an item", async () => {
    const deleteItem = vi.spyOn(repository, "deleteItem");
    render(<App />);
    await screen.findByRole("option", { name: /收到，我整理后今天发给你/ });
    fireEvent.click(await screen.findByRole("button", { name: "删除" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("永久删除这条内容");
    expect(deleteItem).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "永久删除" }));
    await waitFor(() => expect(deleteItem).toHaveBeenCalledWith("1"));
  });

  it("refreshes clipboard changes with the latest active group", async () => {
    const listItems = vi.spyOn(repository, "listItems");
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "常用回复" }));
    await waitFor(() => expect(listItems.mock.calls.some(([options]) => options.groupId === "common")).toBe(true));
    listItems.mockClear();

    await act(async () => { await repository.setPinned("2", true); });
    await waitFor(() => expect(listItems).toHaveBeenCalled());
    expect(listItems.mock.calls.every(([options]) => options.groupId === "common")).toBe(true);
  });

  it("dismisses transient menus and dialogs when the panel is hidden", async () => {
    render(<App />);
    await screen.findByRole("option", { name: /收到，我整理后今天发给你/ });

    fireEvent.click(screen.getByRole("button", { name: "移入分组" }));
    expect(screen.getByText("已选择")).toBeInTheDocument();

    await act(async () => {
      await repository.hidePanel();
    });
    expect(screen.queryByText("已选择")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /新建分组/ }));
    expect(screen.getByText("分组名称")).toBeInTheDocument();

    await act(async () => {
      await repository.hidePanel();
    });
    expect(screen.queryByText("分组名称")).not.toBeInTheDocument();
  });

  it("closes the move menu after moving an item into or out of a group", async () => {
    const moveItem = vi.spyOn(repository, "moveItem");
    render(<App />);
    await screen.findByRole("option", { name: /收到，我整理后今天发给你/ });

    fireEvent.click(screen.getByRole("button", { name: "移入分组" }));
    fireEvent.click(screen.getByRole("button", { name: "移入分组 代码片段" }));
    await waitFor(() => expect(moveItem).toHaveBeenCalledWith("1", "code"));
    expect(screen.queryByText("已选择")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "移入分组" }));
    fireEvent.click(await screen.findByRole("button", { name: "移出分组" }));
    await waitFor(() => expect(moveItem).toHaveBeenCalledWith("1", null));
    expect(screen.queryByRole("button", { name: "移出分组" })).not.toBeInTheDocument();
  });

  it("resets search, group, selection and overlays when the panel is hidden", async () => {
    const listItems = vi.spyOn(repository, "listItems");
    render(<App />);
    const search = await screen.findByLabelText("搜索剪贴板");
    fireEvent.click(screen.getByRole("button", { name: "常用回复" }));
    fireEvent.change(search, { target: { value: "收到" } });
    await waitFor(() => expect(listItems.mock.calls.some(([options]) => options.query === "收到" && options.groupId === "common")).toBe(true));
    fireEvent.click(screen.getByRole("button", { name: /新建分组/ }));
    expect(screen.getByText("分组名称")).toBeInTheDocument();

    await act(async () => {
      await repository.hidePanel();
    });

    expect(search).toHaveValue("");
    expect(screen.queryByText("分组名称")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "最近" })).toHaveClass("active");
    await waitFor(() => expect(listItems.mock.calls.some(([options]) => options.query === "" && options.groupId === null)).toBe(true));
    await waitFor(() => expect(screen.getByRole("option", { name: /收到，我整理后今天发给你/ })).toHaveAttribute("aria-selected", "true"));
  });

  it("renames a clipboard item without changing its content", async () => {
    const renameItem = vi.spyOn(repository, "renameItem");
    render(<App />);
    await screen.findByRole("option", { name: /收到，我整理后今天发给你/ });

    fireEvent.click(await screen.findByRole("button", { name: "修改标题" }));
    const titleInput = screen.getByLabelText("项目标题");
    fireEvent.change(titleInput, { target: { value: "今日待回复" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(renameItem).toHaveBeenCalledWith("1", "今日待回复"));
    expect(await screen.findByRole("option", { name: /今日待回复/ })).toBeInTheDocument();
    expect((await repository.getItem("1")).content).toBe("收到，我整理后今天发给你。");
  });
});
