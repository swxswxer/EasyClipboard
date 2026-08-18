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

  it("pastes a selected item without a copy-only fallback", async () => {
    const pasteItem = vi.spyOn(repository, "pasteItem");
    render(<App />);
    await waitFor(() => expect(screen.getByText("MVP 只保留基础剪贴板功能")).toBeInTheDocument());
    fireEvent.doubleClick(screen.getByText("MVP 只保留基础剪贴板功能"));
    await waitFor(() => expect(pasteItem).toHaveBeenCalledWith("2"));
    expect(screen.queryByText(/已复制/)).not.toBeInTheDocument();
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
    vi.spyOn(repository, "getPermissionState").mockResolvedValue({ clipboard: "authorized", accessibility: false });
    const requestAccessibility = vi.spyOn(repository, "requestAccessibility");
    render(<App />);
    expect(await screen.findByRole("alertdialog")).toHaveTextContent("需要开启辅助功能");
    fireEvent.click(screen.getByRole("button", { name: "开启辅助功能" }));
    await waitFor(() => expect(requestAccessibility).toHaveBeenCalledOnce());
    await waitFor(() => expect(screen.queryByText("需要开启辅助功能")).not.toBeInTheDocument());
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
});
