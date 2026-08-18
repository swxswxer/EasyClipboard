export type ClipboardKind = "text" | "image" | "files";

export type ErrorCode =
  | "not_found"
  | "file_missing"
  | "permission_denied"
  | "shortcut_conflict"
  | "content_too_large"
  | "clipboard_unavailable"
  | "storage_error";

export interface ClipboardItemSummary {
  id: string;
  kind: ClipboardKind;
  title: string;
  sourceName: string;
  sourceBundleId: string | null;
  copiedAt: string;
  byteSize: number;
  pinned: boolean;
  groupId: string | null;
  retained: boolean;
  missingFiles: boolean;
}

export interface ClipboardItemDetail extends ClipboardItemSummary {
  content: string;
  previewDataUrl: string | null;
  files: string[];
}

export interface ClipboardPage {
  items: ClipboardItemSummary[];
  nextCursor: string | null;
}

export interface Group {
  id: string;
  name: string;
  sortOrder: number;
  createdAt: string;
}

export interface ExcludedApp {
  name: string;
  bundleId: string;
}

export interface Settings {
  shortcut: string;
  launchAtLogin: boolean;
  recordingPaused: boolean;
  maxItems: 100 | 500 | 1000 | 5000;
  retentionDays: 0 | 7 | 30 | 90;
  excludedApps: ExcludedApp[];
}

export interface PermissionState {
  clipboard: "not_requested" | "authorized" | "denied";
  accessibility: boolean;
}

export interface RepositoryErrorShape {
  code: ErrorCode;
  message: string;
}

export class RepositoryError extends Error {
  code: ErrorCode;

  constructor(error: RepositoryErrorShape | ErrorCode, message?: string) {
    const code = typeof error === "string" ? error : error.code;
    super(typeof error === "string" ? (message ?? code) : error.message);
    this.name = "RepositoryError";
    this.code = code;
  }
}
