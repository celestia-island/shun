/**
 * The shell's Tauri surface, on EXPLICIT imports.
 *
 * The installer ships `app.withGlobalTauri: false`, so no `window.__TAURI__`
 * object is injected: every call goes through the packaged `@tauri-apps/api`
 * and `@tauri-apps/plugin-dialog` modules instead. Keeping the global object
 * off the page removes a script-visible handle that any injected asset could
 * drive directly; the ipc core (`window.__TAURI_INTERNALS__`) stays, because
 * that IS the transport, and the configured CSP's `script-src 'self'` keeps
 * its only users on our own bundle.
 *
 * The guards below keep the browser preview working: `dist/` served outside
 * the shell (vite preview, the `?product=`/`?version=` preview hook) has no
 * ipc core, and every entry point then degrades to "not available" instead
 * of throwing at mount time.
 */

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { getCurrentWindow, type Window } from "@tauri-apps/api/window";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

/** The caption-control surface the title bar drives — the packaged window
 *  handle, named for what the call sites use it as. */
export type TauriWindow = Window;

/** Whether the page runs inside the shell's webview: tauri injects its ipc
 *  core into every shell page, whether or not the global API is enabled. */
function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function tauriWindow(): TauriWindow | null {
  return inTauri() ? getCurrentWindow() : null;
}

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri()) throw new Error("Tauri API 不可用");
  return tauriInvoke<T>(cmd, args);
}

export async function listen<T>(event: string, handler: (payload: T) => void): Promise<void> {
  if (!inTauri()) return;
  await tauriListen<T>(event, (e) => handler(e.payload));
}

export async function openDirectory(title: string): Promise<string | null> {
  if (!inTauri()) return null;
  return openDialog({ directory: true, title });
}