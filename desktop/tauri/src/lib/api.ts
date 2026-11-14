// Client for the app's backend. The routes are served in-process by the
// ModSync engine through the Tauri `api_request` command, and live events
// arrive as the `ws-event` window event.
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "./tauri";

export interface GameFrontRequest {
  id: string;
  url: string;
  destination: string;
  name: string;
}


export type ModStatus =
  | "PENDING" | "DOWNLOADING" | "EXTRACTING" | "READY" | "INSTALLING"
  | "WAITING_PATCHER" | "DONE" | "SKIPPED" | "MANUAL" | "ERROR";

export interface BuildInfo {
  key: string;
  label: string;
  game: string;
  custom?: boolean;
  url?: string;
}

export interface BuildMod {
  install_order: number;
  file_id: string;
  slug: string;
  name: string;
  url: string;
  game: string;
  section: string;
  category: string;
  note: string;
  option_hint: string;
  install_method_hint: string;
  build_key: string;
  // Full per-mod detail parsed from the build guide, and a short summary of the
  // special handling the installer applies (see installer/build_directives.py).
  instructions?: string;
  warnings?: string;
  install_method?: string;
  description?: string;
  author?: string;
  directive_summary?: string;
  /** Where the mod is hosted, and whether the app can fetch it unattended. */
  source_host?: string;
  source_label?: string;
  auto_downloadable?: boolean;
  patch_urls?: string[];
  installed?: boolean;
}

export interface AppStatus {
  version: string;
  logged_in: boolean;
  pipeline_running: boolean;
  shim_available: boolean;
  shim_path: string | null;
  current_build: string;
  /** Whether a Nexus API key is saved (Nexus mods fail without one). */
  nexus_key_set?: boolean;
}

export interface Settings {
  kotor1_path: string;
  kotor2_path: string;
  download_dir: string;
  language: string;
  custom_patcher_path: string;
  seven_zip_path: string;
  nexus_api_key: string;
  filter_downloads_by_resolution: boolean;
}

export interface NexusValidation {
  ok: boolean;
  name?: string;
  is_premium?: boolean;
  /** What Nexus last reported about this account's allowance (absent if not sent). */
  limits?: { hourly_remaining?: number; daily_remaining?: number };
  error?: string;
}

export interface PatcherStatus {
  available: boolean;
  path: string | null;
  source: "bundled" | "custom" | "system" | "none";
  custom_patcher_path: string;
  strategies: string[];
}

export type GameKey = "KOTOR1" | "KOTOR2";

export interface Profile {
  id: string;
  name: string;
  game: GameKey;
  path: string;
  is_default: boolean;
}

export interface ModInfo {
  title: string;
  description: string;
  images: string[];
  author: string;
  ds_url: string;
  nexus_url: string;
  error?: string;
}

export interface DeployedFile { rel_path: string; sha256: string; size: number; overwrote: boolean; }
export interface BakedFile { rel_path: string; post_sha256: string; created: boolean; }

export interface LibraryMod {
  id: string; name: string; game: GameKey; enabled: boolean; toggleable: boolean;
  state: string; install_method: string; deploy_kind: string; load_order: number;
  source_type: string; source_ref: string; source_slug: string; build_key: string | null;
  source_host?: string; source_url?: string;
  category: string;
  file_count: number; baked_count: number; install_ts: number;
  has_conflict: boolean; conflict_count: number;
  source_exists: boolean;
  /** Shares a name with another entry but cannot safely be merged with it. */
  duplicate_unmergeable?: boolean;
}

export type LibraryDetail = LibraryMod & {
  deployed_files: DeployedFile[];
  baked_files: BakedFile[];
};

export interface ConflictParticipant {
  mod_id: string; mod_name: string; enabled: boolean;
  build_key?: string | null;
  logical_id?: string;
  // Patcher mods bake their changes into shared files and cannot be switched
  // off, so offering a Disable button for them only ever fails.
  toggleable?: boolean;
  install_method?: string;
  // What this mod writes, so a conflict can show evidence rather than assert.
  files?: string[];
  file_count?: number;
}

export interface Conflict {
  id: string; resource: string;
  type: "override" | "2da" | "dialog" | "module" | "declared";
  severity: "info" | "warning" | "error"; participants: ConflictParticipant[];
  winner_mod_id: string | null;
  same_build?: boolean;
  description: string;
  recommendation: string;
  // Files both mods actually write (declared incompatibilities only).
  shared_files?: string[];
  shared_file_count?: number;
}

// WebSocket event shapes
export type WsEvent =
  | { type: "hello"; version: string }
  | { type: "auth"; logged_in: boolean; username: string }
  | { type: "log"; message: string; tag: string }
  | { type: "nxm_wait"; active: boolean }
  | { type: "confirm"; id: string; title: string; body: string; options: string[] }
  | { type: "status"; file_id: string; status: ModStatus; status_label: string; detail: string }
  | { type: "progress"; file_id: string; pct: number; kb: number; total_kb: number }
  | { type: "install_progress"; file_id: string; pct: number; label: string }
  | { type: "manual"; file_id: string; name: string; folder: string; readme: string }
  | { type: "update_progress"; pct: number; downloaded: number; total: number }
  | { type: "library"; event: "import_folder_done"; count: number; [k: string]: unknown }
  | { type: "library"; event: "changed"; [k: string]: unknown }
  | { type: "library"; event: "bulk_progress"; action: string; current: number;
      total: number; mod: string; done?: boolean }
  | { type: "pipeline"; event: "started" | "finished"; [k: string]: unknown };

async function req<T>(path: string, init?: RequestInit): Promise<T> {
  try {
    return await invoke<T>("api_request", {
      path,
      method: init?.method ?? "GET",
      body: typeof init?.body === "string" ? JSON.parse(init.body) : null,
    });
  } catch (e: any) {
    const data = typeof e === "object" && e ? e : { error: String(e) };
    throw Object.assign(new Error(data?.error || "Request failed"), {
      status: data?.status,
      data,
    });
  }
}

export const api = {
  health: () => req<{ ok: boolean; version: string; platform?: "windows" | "linux" | "macos" }>("/api/health"),
  detectGames: () =>
    req<{ installs: { game: GameKey; path: string; source: string; already_added: boolean }[] }>(
      "/api/games/detect"),
  status: () => req<AppStatus>("/api/status"),
  answerConfirm: (id: string, choice: string) =>
    req<{ ok: boolean }>("/api/install/confirm", {
      method: "POST",
      body: JSON.stringify({ id, choice }),
    }),
  builds: () => req<{ builds: BuildInfo[] }>("/api/builds"),
  addBuild: (label: string, game: string, url: string) =>
    req<{ ok: boolean; build: BuildInfo }>("/api/builds", {
      method: "POST",
      body: JSON.stringify({ label, game, url }),
    }),
  addLocalBuild: (path: string) =>
    req<{ ok: boolean; build: BuildInfo }>("/api/builds/local", {
      method: "POST",
      body: JSON.stringify({ path }),
    }),
  deleteBuild: (key: string) =>
    req<{ ok: boolean }>(`/api/builds/${encodeURIComponent(key)}`, { method: "DELETE" }),
  sourceSite: () =>
    req<{ url: string; username: string; has_password: boolean }>("/api/source-site"),
  saveSourceSite: (body: { url?: string; username?: string; password?: string; clear_password?: boolean }) =>
    req<{ ok: boolean; has_password: boolean }>("/api/source-site", {
      method: "POST",
      body: JSON.stringify(body),
    }),
  credentials: () => req<{ username: string }>("/api/credentials"),
  login: (username: string, password: string, save = true) =>
    req<{ ok: boolean; username: string }>("/api/login", {
      method: "POST",
      body: JSON.stringify({ username, password, save }),
    }),
  logout: () => req<{ ok: boolean }>("/api/logout", { method: "POST" }),
  nexusValidate: () => req<NexusValidation>("/api/nexus/validate"),
  getSettings: () => req<Settings>("/api/settings"),
  extractorStatus: () => req<{ seven_zip_path: string | null }>("/api/extractor/status"),
  setSettings: (s: Settings) =>
    req<{ ok: boolean }>("/api/settings", { method: "POST", body: JSON.stringify(s) }),
  /** refresh forces a fresh fetch of the guide instead of using the cache. */
  loadBuild: (key: string, profile?: string, refresh = false) => {
    const q = new URLSearchParams();
    if (profile) q.set("profile", profile);
    if (refresh) q.set("refresh", "true");
    const qs = q.toString();
    return req<{ ok: boolean; build_key: string; from_cache?: boolean;
                 mods: BuildMod[] }>(
      `/api/builds/${key}/load${qs ? `?${qs}` : ""}`, { method: "POST" });
  },
  startInstall: (
    build_key: string,
    game_path?: string,
    selected_file_ids?: string[],
  ) =>
    req<{ ok: boolean; total: number; game_path: string }>("/api/install/start", {
      method: "POST",
      body: JSON.stringify({ build_key, unattended: true, game_path, selected_file_ids,
        gamefront_browser: isTauri() }),
    }),
  gamefrontRequest: () => req<{ request: GameFrontRequest | null }>("/api/gamefront/request"),
  gamefrontExternal: () => req<{ ok: boolean; destination: string }>("/api/gamefront/external", { method: "POST" }),
  control: (action: "pause" | "resume" | "stop" | "retry") =>
    req<{ ok: boolean; action: string }>(`/api/install/${action}`, { method: "POST" }),
  recovery: () =>
    req<{ pending: boolean; game_path?: string; mods_path?: string }>("/api/recovery"),
  restore: () => req<{ ok: boolean }>("/api/recovery/restore", { method: "POST" }),
  installState: () =>
    req<{ running: boolean; mods: any[] }>("/api/install/state"),
  // Profiles (multiple game installs on one machine).
  profiles: () =>
    req<{ profiles: Profile[]; active: string }>("/api/profiles"),
  createProfile: (body: { name: string; game: GameKey; path: string }) =>
    req<{ ok: boolean; profile: Profile }>("/api/profiles", {
      method: "POST",
      body: JSON.stringify(body),
    }),
  updateProfile: (id: string, body: { name?: string; path?: string }) =>
    req<{ ok: boolean; profile: Profile }>(`/api/profiles/${id}`, {
      method: "PATCH",
      body: JSON.stringify(body),
    }),
  deleteProfile: (id: string) =>
    req<{ ok: boolean }>(`/api/profiles/${id}`, { method: "DELETE" }),
  setActiveProfile: (id: string) =>
    req<{ ok: boolean }>("/api/profiles/active", {
      method: "POST",
      body: JSON.stringify({ id }),
    }),

  library: (profile: string) =>
    req<{ game: string; profile: string; mods: LibraryMod[]; mergeable_duplicates?: number }>(
      `/api/library?profile=${encodeURIComponent(profile)}`),
  libraryDetail: (id: string, profile: string) =>
    req<LibraryDetail>(
      `/api/library/${id}?profile=${encodeURIComponent(profile)}`),
  libraryEnable: (profile: string, id: string) =>
    req<{ ok: boolean; mod?: LibraryMod; conflicts?: Conflict[] }>(
      `/api/library/${id}/enable?profile=${encodeURIComponent(profile)}`, { method: "POST" }),
  libraryDisable: (profile: string, id: string) =>
    req<{ ok: boolean; mod?: LibraryMod; conflicts?: Conflict[] }>(
      `/api/library/${id}/disable?profile=${encodeURIComponent(profile)}`, { method: "POST" }),
  libraryUninstall: (profile: string, id: string, force = false) =>
    req<{ ok: boolean }>(
      `/api/library/${id}/uninstall?profile=${encodeURIComponent(profile)}`, {
        method: "POST",
        body: JSON.stringify({ force }),
      }),
  libraryOpenFolder: (profile: string, id: string) =>
    req<{ ok: boolean; path?: string }>(
      `/api/library/${id}/open-folder?profile=${encodeURIComponent(profile)}`, { method: "POST" }),
  openPath: (path: string, select = false) =>
    req<{ ok: boolean }>("/api/open-path", {
      method: "POST",
      body: JSON.stringify({ path, select }),
    }),
  openDownloadFolder: (file_id: string, slug: string, game: string) =>
    req<{ ok: boolean; path?: string; fallback?: boolean }>("/api/mod/open-download", {
      method: "POST",
      body: JSON.stringify({ file_id, slug, game }),
    }),
  conflicts: (profile: string) =>
    req<{ conflicts: Conflict[] }>(
      `/api/conflicts?profile=${encodeURIComponent(profile)}`),
  resolveConflicts: (
    profile: string,
    body: { action: "dismiss" | "undismiss" | "disable_losers";
            conflict_ids?: string[]; all_of_severity?: string },
  ) =>
    req<{ ok: boolean; requested: number; dismissed: number;
          disabled: string[]; skipped: { id: string; reason: string }[] }>(
      `/api/conflicts/resolve?profile=${encodeURIComponent(profile)}`,
      { method: "POST", body: JSON.stringify(body) }),
  bulkUninstall: (profile: string, mod_ids: string[], force = false) =>
    req<{ ok: boolean; removed: string[];
          failed: { mod: string; reason: string }[]; requested: number }>(
      `/api/library/bulk/uninstall?profile=${encodeURIComponent(profile)}`,
      { method: "POST", body: JSON.stringify({ mod_ids, force }) }),
  cacheStats: (profile: string) =>
    req<{ path: string; total_bytes: number; count: number; in_use_bytes: number;
          entries: { name: string; bytes: number; in_use: boolean; files: number }[] }>(
      `/api/cache?profile=${encodeURIComponent(profile)}`),
  openCache: (profile: string, name = "") =>
    req<{ ok: boolean; path?: string }>(
      `/api/cache/open?profile=${encodeURIComponent(profile)}`
      + `&name=${encodeURIComponent(name)}`, { method: "POST" }),
  clearCache: (profile: string, opts: { keep_installed?: boolean; names?: string[] }) =>
    req<{ ok: boolean; removed: string[]; freed_bytes: number;
          failed: { name: string; reason: string }[] }>(
      `/api/cache/clear?profile=${encodeURIComponent(profile)}`,
      { method: "POST", body: JSON.stringify({
          keep_installed: opts.keep_installed ?? true, names: opts.names ?? [] }) }),
  baselineStatus: (profile: string) =>
    req<{ has_baseline: boolean; game_path: string }>(
      `/api/library/baseline?profile=${encodeURIComponent(profile)}`),
  baselineReset: (profile: string) =>
    req<{ ok: boolean; override_removed: number; modules_removed: number;
          restored: string[] }>(
      `/api/library/baseline/reset?profile=${encodeURIComponent(profile)}`,
      { method: "POST" }),
  dedupeLibrary: (profile: string) =>
    req<{ ok: boolean; removed: number; mods: string[]; remaining: number }>(
      `/api/library/dedupe?profile=${encodeURIComponent(profile)}`,
      { method: "POST" }),
  bulkToggle: (profile: string, mod_ids: string[], action: "enable" | "disable") =>
    req<{ ok: boolean; action: string; changed: string[];
          failed: { mod: string; reason: string }[] }>(
      `/api/library/bulk/toggle?profile=${encodeURIComponent(profile)}&action=${action}`,
      { method: "POST", body: JSON.stringify({ mod_ids }) }),
  importFolder: (body: { game: string; path: string; profile?: string }) =>
    req<{ ok: boolean; count: number }>("/api/library/import-folder", {
      method: "POST",
      body: JSON.stringify({ ...body, unattended: true }),
    }),
  importMod: (body: { game: string; path: string; profile?: string }) =>
    req<{ ok: boolean; detected_method?: string }>("/api/library/import", {
      method: "POST",
      body: JSON.stringify({ ...body, unattended: true }),
    }),
  modInfo: (fileId: string, slug: string, game: string) =>
    req<ModInfo>(
      `/api/mod/info?file_id=${encodeURIComponent(fileId)}&slug=${encodeURIComponent(slug)}&game=${game}`),
  imageProxy: (url: string) => convertFileSrc(url, "kmm-image"),
  spoilers: () => req<{ spoiler_free: boolean }>("/api/spoilers"),
  setSpoilers: (spoiler_free: boolean) =>
    req<{ ok: boolean }>("/api/spoilers", { method: "POST", body: JSON.stringify({ spoiler_free }) }),
  patcherStatus: () => req<PatcherStatus>("/api/patcher/status"),
  patcherOptions: () => req<{ engine: string; allow_programs: boolean }>("/api/patcher/options"),
  savePatcherOptions: (body: { engine: string; allow_programs: boolean }) =>
    req<{ ok: boolean }>("/api/patcher/options", { method: "POST", body: JSON.stringify(body) }),
  openUrl: (url: string) =>
    req<{ ok: boolean }>(
      `/api/update/open?url=${encodeURIComponent(url)}`, { method: "POST" }),
  updateCheck: () => req<UpdateInfo>("/api/update/check"),
  updateOpen: (url?: string) =>
    req<{ ok: boolean }>(`/api/update/open${url ? `?url=${encodeURIComponent(url)}` : ""}`, {
      method: "POST",
    }),
  updateDownload: () =>
    req<{ ok: boolean; path?: string; version?: string }>("/api/update/download", { method: "POST" }),
  whatsNew: () => req<{ version: string; notes: string; show: boolean }>("/api/whatsnew"),
  whatsNewSeen: () => req<{ ok: boolean }>("/api/whatsnew/seen", { method: "POST" }),
  setupState: () => req<{ show: boolean }>("/api/setup"),
  setupDone: () => req<{ ok: boolean }>("/api/setup/done", { method: "POST" }),
};

export interface UpdateInfo {
  available: boolean;
  current_version: string;
  latest_version?: string;
  url?: string | null;
  asset_url?: string | null;
  notes?: string;
  repo?: string;
  /** False in the Nexus Mods build: it only announces updates. */
  can_self_update?: boolean;
  nexus_url?: string;
  error?: string;
}

// Live events from the backend.
export function connectEvents(
  onEvent: (e: WsEvent) => void,
  onOpen?: () => void,
  onClose?: () => void,
): () => void {
  let closed = false;
  let unlisten: (() => void) | null = null;
  listen<WsEvent>("ws-event", (m) => onEvent(m.payload))
    .then((un) => { if (closed) un(); else { unlisten = un; onOpen?.(); } })
    .catch(() => onClose?.());
  return () => {
    closed = true;
    unlisten?.();
    onClose?.();
  };
}

// Wait for the backend to come up (sidecar may still be starting).
export async function waitForBackend(timeoutMs = 30000): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      await api.health();
      return true;
    } catch {
      await new Promise((r) => setTimeout(r, 400));
    }
  }
  return false;
}
