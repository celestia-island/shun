/**
 * Minimal access to the `withGlobalTauri` global API. The installer shell
 * enables `app.withGlobalTauri`, so no npm @tauri-apps packages are needed
 * for this tiny frontend.
 */

export interface TauriWindow {
  isMaximized(): Promise<boolean>;
  onResized(handler: () => void): Promise<() => void>;
  minimize(): Promise<void>;
  toggleMaximize(): Promise<void>;
  close(): Promise<void>;
  startDragging(): Promise<void>;
}

interface TauriGlobal {
  core?: { invoke?: (cmd: string, args?: Record<string, unknown>) => Promise<unknown> };
  event?: { listen?: (event: string, handler: (e: { payload: unknown }) => void) => Promise<() => void> };
  dialog?: { open?: (options?: { directory?: boolean; title?: string }) => Promise<string | string[] | null> };
  window?: { getCurrentWindow?: () => TauriWindow };
}

function tauri(): TauriGlobal | null {
  return (window as unknown as { __TAURI__?: TauriGlobal }).__TAURI__ ?? null;
}

export function tauriWindow(): TauriWindow | null {
  return tauri()?.window?.getCurrentWindow?.() ?? null;
}

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const invoke = tauri()?.core?.invoke;
  if (!invoke) throw new Error("Tauri API 不可用");
  return invoke(cmd, args) as Promise<T>;
}

export async function listen<T>(event: string, handler: (payload: T) => void): Promise<void> {
  const listen = tauri()?.event?.listen;
  if (!listen) return;
  await listen(event, (e) => handler(e.payload as T));
}

/**
 * Pick a directory for the install-target field.
 *
 * This is the picker seam the wizard's directory row sits on — the
 * field component is a candidate hikari file picker, whose backends
 * are (a) the browser-native picker, (b) a hikari in-app picker, and
 * (c) a host-supplied hook. Here the auto chain resolves to:
 *
 *   1. custom hook — the Tauri2 dialog plugin. The OS window it opens
 *      lives outside the webview (Tauri2 has no in-app dialog), so it
 *      counts as the app hook backend rather than the browser one;
 *   2. browser native is deliberately skipped for install targets:
 *      `showDirectoryPicker()` resolves to an opaque handle whose only
 *      readable property is the leaf name — no absolute path, so there
 *      is nothing truthful to put in the field;
 *   3. none — null, and the user types the path by hand.
 */
export async function openDirectory(title: string): Promise<string | null> {
  const open = tauri()?.dialog?.open;
  if (!open) return null;
  const picked = await open({ directory: true, title });
  return typeof picked === "string" ? picked : null;
}
