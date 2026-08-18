import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TestClipboardRepository } from "../test/TestClipboardRepository";
import { SettingsPage } from "./SettingsPage";

describe("SettingsPage", () => {
  it("captures a new shortcut at window level and saves it", async () => {
    const repository = new TestClipboardRepository();
    const setGlobalShortcut = vi.spyOn(repository, "setGlobalShortcut");
    render(<SettingsPage repository={repository} />);
    const recorder = await screen.findByRole("button", { name: "⌘ ⇧ V" });
    fireEvent.click(recorder);
    fireEvent.keyDown(window, { key: "p", code: "KeyP", metaKey: true, shiftKey: true });
    await waitFor(() => expect(setGlobalShortcut).toHaveBeenCalledWith("Command+Shift+P"));
    expect(await screen.findByText("快捷键已更新")).toBeInTheDocument();
  });

  it("closes through the repository window command", async () => {
    const repository = new TestClipboardRepository();
    const closeSettings = vi.spyOn(repository, "closeSettings").mockResolvedValue();
    render(<SettingsPage repository={repository} />);
    fireEvent.click(await screen.findByRole("button", { name: "关闭设置" }));
    expect(closeSettings).toHaveBeenCalledOnce();
  });

  it("clears ordinary history after an in-app confirmation", async () => {
    const repository = new TestClipboardRepository();
    const clearRecent = vi.spyOn(repository, "clearRecent");
    render(<SettingsPage repository={repository} />);
    fireEvent.click(await screen.findByRole("button", { name: /清空普通历史/ }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("固定内容和分组中的内容会继续保留");
    expect(clearRecent).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "清空历史" }));
    await waitFor(() => expect(clearRecent).toHaveBeenCalledOnce());
    expect(await screen.findByText("普通历史已清空")).toBeInTheDocument();
  });

  it("deletes all local data only after the two in-app confirmation steps", async () => {
    const repository = new TestClipboardRepository();
    const deleteAllData = vi.spyOn(repository, "deleteAllData");
    render(<SettingsPage repository={repository} />);
    fireEvent.click(await screen.findByRole("button", { name: /删除全部本地数据/ }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("删除全部本地数据");
    fireEvent.click(screen.getByRole("button", { name: "继续" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("最后确认");
    expect(deleteAllData).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "永久删除" }));
    await waitFor(() => expect(deleteAllData).toHaveBeenCalledOnce());
    expect(await screen.findByText("全部本地数据已删除")).toBeInTheDocument();
  });

  it("uses Windows shortcut labels and hides macOS-only settings", async () => {
    const repository = new TestClipboardRepository();
    vi.spyOn(repository, "getDesktopCapabilities").mockResolvedValue({
      platform: "windows", clipboardAccess: "ready", pasteAutomation: "ready", supportsAppExclusions: false,
    });
    vi.spyOn(repository, "getSettings").mockResolvedValue({
      shortcut: "Control+Shift+V", launchAtLogin: false, recordingPaused: false,
      maxItems: 500, retentionDays: 30, excludedApps: [],
    });
    render(<SettingsPage repository={repository} />);
    expect(await screen.findByRole("button", { name: "Ctrl Shift V" })).toBeInTheDocument();
    expect(screen.queryByText("辅助功能权限")).not.toBeInTheDocument();
    expect(screen.queryByText("排除应用")).not.toBeInTheDocument();
    expect(screen.getByText(/系统托盘/)).toBeInTheDocument();
  });
});
