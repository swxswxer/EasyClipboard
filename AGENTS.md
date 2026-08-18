# EasyClipboard Instructions

Build the React UI in `src/` and the Tauri desktop layer in `src-tauri/`. Keep production code Tauri-only; browser mock repositories and prototype hosting paths do not belong in the application.

## Product decisions

- Preserve the selected search-first layout: clipboard result list on the left and selected-item preview on the right.
- Keep one primary panel near the bottom edge and use a single neutral dark frosted-glass visual system on both Windows and macOS. Do not maintain platform-specific visual modes or side-by-side examples.
- Groups are first-class MVP functionality. Users can create, rename, and delete one-level groups, move clipboard items into or out of them, and search within the active group.
- Items inside a group are permanently retained and must not be removed by ordinary history cleanup. Only an explicit user delete removes grouped content.
- Deleting a group moves its contents back to Recent by default instead of deleting them.
- Accessibility permission is mandatory for clipboard recording, history access, and paste; there is no copy-only fallback or optional auto-paste mode.
- Group tabs use direct controls: clicking the active group name renames it, and a compact × control deletes it after an in-app confirmation. Do not use an ellipsis management menu.
- Clipboard navigation is window-wide while the main panel is active: Arrow Up/Down and Return must keep working after mouse clicks on groups, controls, previews, or empty areas. Only modal, menu, permission, text-composition, and explicit Tab-navigation scopes may temporarily own those keys.
- Every shortcut-driven panel reveal must restore focus to the search field so typing starts a search immediately. Keyboard-selected rows use the same single selection highlight as mouse-selected rows; do not layer a native focus outline on top of that highlight.
- The global clipboard shortcut toggles the panel: it shows a hidden panel and hides a visible panel. Destructive settings actions use visible in-app confirmation dialogs with success or failure feedback, not native `window.confirm` dialogs.
