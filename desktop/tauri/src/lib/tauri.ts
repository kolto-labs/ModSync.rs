// Thin wrappers around Tauri APIs that degrade gracefully when running in a
// plain browser (e.g. `npm run dev` without the Tauri shell).

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const GAMEFRONT_OPEN_TIMEOUT_MS = 20_000;

export async function openGameFront(request: {
  id: string; url: string; destination: string; name: string;
} | null = null): Promise<void> {
  if (!isTauri()) throw new Error("The saved GameFront session requires the desktop app.");
  const { invoke } = await import("@tauri-apps/api/core");
  // A browser window that never answers must not freeze the installer. Give up
  // waiting so the caller can fall back to the player's regular browser.
  let timer: number | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = window.setTimeout(
      () => reject(new Error("The GameFront window did not respond.")),
      GAMEFRONT_OPEN_TIMEOUT_MS,
    );
  });
  try {
    await Promise.race([invoke("open_gamefront", { request }), timeout]);
  } finally {
    window.clearTimeout(timer);
  }
}

/** Force the GameFront window closed, even if its page is stuck. */
export async function closeGameFront(): Promise<void> {
  if (!isTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("close_gamefront");
}

/** Whether the player is signed in to GameFront (desktop app only). */
export async function gameFrontSignedIn(): Promise<boolean> {
  if (!isTauri()) return false;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<boolean>("gamefront_account");
}

/** Forget the saved GameFront sign-in. */
export async function signOutGameFront(): Promise<void> {
  if (!isTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("sign_out_gamefront");
}

export async function finishGameFront(id: string): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("finish_gamefront", { id });
}

/** Invoke the Rust apply_update command (download swap + relaunch). */
export async function applyUpdate(newPath: string): Promise<boolean> {
  if (!isTauri()) return false;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("apply_update", { newPath });
    return true;
  } catch {
    return false;
  }
}

/** Claim (or give back) nxm:// links. Only claimed while a free Nexus account's
 * download waits for its click, so other Nexus mod managers keep their links. */
export async function setNxmHandler(enabled: boolean): Promise<void> {
  if (!isTauri()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("set_nxm_handler", { enabled });
  } catch {
    // Best effort: the download falls back to timing out with a clear message.
  }
}

/** Folder picker. `title` names what the player is choosing (the OS default is just "Select Folder"). */
export async function pickDirectory(title?: string): Promise<string | null> {
  if (!isTauri()) {
    // Browser fallback: prompt for a path.
    const p = window.prompt(title ?? "Enter folder path:");
    return p && p.trim() ? p.trim() : null;
  }
  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const result = await open({ directory: true, multiple: false, title });
    return typeof result === "string" ? result : null;
  } catch {
    return null;
  }
}

/**
 * Subscribe to OS file/folder drag-drop onto the webview. Returns an
 * unsubscribe function. In a plain browser this is a no-op.
 */
export async function onFilesDropped(
  cb: (paths: string[]) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  try {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    const unlisten = await getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "drop") cb(e.payload.paths);
    });
    return unlisten;
  } catch {
    return () => {};
  }
}

/**
 * Subscribe to drag-drop hover state (enter/over vs leave/drop) so the UI can
 * highlight a drop zone. Returns an unsubscribe function; no-op in a browser.
 */
export async function onDragHover(
  cb: (active: boolean) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  try {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    const unlisten = await getCurrentWebview().onDragDropEvent((e) => {
      const type = e.payload.type;
      if (type === "enter" || type === "over") cb(true);
      else cb(false);
    });
    return unlisten;
  } catch {
    return () => {};
  }
}

/** Pick an instruction file (TOML, JSON, YAML, XML or Markdown). */
export async function pickInstructionFile(title?: string): Promise<string | null> {
  if (!isTauri()) {
    const p = window.prompt(title ?? "Enter file path:");
    return p && p.trim() ? p.trim() : null;
  }
  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const result = await open({
      multiple: false,
      directory: false,
      title,
      filters: [{ name: "Instruction file", extensions: ["toml", "json", "yaml", "yml", "xml", "md", "markdown"] }],
    });
    return typeof result === "string" ? result : null;
  } catch {
    return null;
  }
}

/** Pick a single executable file. Pass [] to include extensionless Linux tools. */
export async function pickFile(
  extensions: string[] = ["exe"],
): Promise<string | null> {
  if (!isTauri()) {
    const p = window.prompt("Enter file path:");
    return p && p.trim() ? p.trim() : null;
  }
  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const result = await open({
      multiple: false,
      directory: false,
      filters: extensions.length ? [{ name: "Executable", extensions }] : undefined,
    });
    return typeof result === "string" ? result : null;
  } catch {
    return null;
  }
}
