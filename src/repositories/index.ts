import type { ClipboardRepository } from "../repository";
import { TauriClipboardRepository } from "./tauri";

export const repository: ClipboardRepository = new TauriClipboardRepository();
