import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  api, connectEvents, waitForBackend,
  type AppStatus, type BuildInfo, type BuildMod, type Profile, type WsEvent,
} from "@/lib/api";
import { DEFAULT_RUNTIME, type ModRuntime } from "@/components/ModList";
import { type LogLine } from "@/components/LogPanel";
import { LoginDialog } from "@/components/LoginDialog";
import { WhatsNew } from "@/components/WhatsNew";
import { SetupWizard } from "@/components/SetupWizard";
import { ConfirmDialog, type ConfirmRequest } from "@/components/ConfirmDialog";
import { AppShell } from "@/layouts/AppShell";
import { useT } from "@/lib/i18n";
import { setNxmHandler } from "@/lib/tauri";
import { useGameFront } from "@/lib/useGameFront";
import { BuildsView } from "@/views/BuildsView";
import { LibraryView } from "@/views/LibraryView";
import { ConflictsView } from "@/views/ConflictsView";
import { ActivityView } from "@/views/ActivityView";
import { SettingsView, type SettingsSectionId } from "@/views/SettingsView";
import { type ViewId } from "@/lib/views";

const FINAL = new Set(["DONE", "SKIPPED", "ERROR"]);

const VIEW_IDS: ViewId[] = ["builds", "library", "conflicts", "activity", "settings"];

export default function App() {
  const t = useT();
  const [view, setViewState] = useState<ViewId>(() => {
    const h = typeof location !== "undefined" ? (location.hash.replace("#", "") as ViewId) : "builds";
    return VIEW_IDS.includes(h) ? h : "builds";
  });
  const [visited, setVisited] = useState(() => new Set<ViewId>([view]));
  const setView = useCallback((v: ViewId) => {
    setVisited(prev => prev.has(v) ? prev : new Set([...prev, v]));
    setViewState(v);
    if (typeof location !== "undefined") location.hash = v;
  }, []);
  useEffect(() => {
    const onHashChange = () => {
      const next = (location.hash.slice(1) || "builds") as ViewId;
      if (VIEW_IDS.includes(next)) {
        setVisited(prev => prev.has(next) ? prev : new Set([...prev, next]));
        setViewState(next);
      }
    };
    window.addEventListener("hashchange", onHashChange);
    return () => window.removeEventListener("hashchange", onHashChange);
  }, []);

  // Which Settings sub-section is open, so other parts of the UI (e.g. clicking
  // the signed-in profile) can jump straight to it.
  const [settingsSection, setSettingsSection] = useState<SettingsSectionId>("general");
  const openSettingsSection = useCallback((s: SettingsSectionId) => {
    setSettingsSection(s);
    setView("settings");
  }, [setView]);

  const [ready, setReady] = useState(false);
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [builds, setBuilds] = useState<BuildInfo[]>([]);
  const [selectedBuild, setSelectedBuild] = useState("k1_full");
  const [mods, setMods] = useState<BuildMod[]>([]);
  const [runtime, setRuntime] = useState<Record<string, ModRuntime>>({});
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [running, setRunning] = useState(false);
  const [paused, setPaused] = useState(false);
  const [patcherMod, setPatcherMod] = useState<string | null>(null);
  const [showLogin, setShowLogin] = useState(false);
  const [confirmReq, setConfirmReq] = useState<ConfirmRequest | null>(null);
  const [username, setUsername] = useState("");
  const [conflictCount, setConflictCount] = useState(0);
  // Live per-mod progress for a running bulk action, so the Library screen can
  // name each mod as it goes instead of showing one static message.
  const [bulkProgress, setBulkProgress] = useState<
    { action: string; current: number; total: number; mod: string } | null
  >(null);
  const [dataTick, setDataTick] = useState(0);
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [activeProfile, setActiveProfile] = useState<string>("");
  const [whatsNew, setWhatsNew] = useState<{ version: string; notes: string } | null>(null);
  const [showSetup, setShowSetup] = useState(false);

  const logId = useRef(0);
  const addLog = useCallback((message: string, tag = "") => {
    setLogs((prev) => {
      const next = [...prev, { id: logId.current++, message, tag, ts: Date.now() }];
      return next.length > 600 ? next.slice(-600) : next;
    });
  }, []);

  const resetRuntime = useCallback(() => setRuntime({}), []);
  useGameFront(ready, addLog);

  const refreshStatus = useCallback(async () => {
    try {
      const s = await api.status();
      setStatus(s);
      setRunning(s.pipeline_running);
    } catch { /* backend not up yet */ }
  }, []);

  // Keep a ref of the active profile so closures (WS handler) see the latest.
  const activeProfileRef = useRef(activeProfile);
  useEffect(() => { activeProfileRef.current = activeProfile; }, [activeProfile]);

  const refreshConflicts = useCallback(async () => {
    const pid = activeProfileRef.current;
    if (!pid) return;
    try {
      const r = await api.conflicts(pid);
      setConflictCount(r.conflicts?.length ?? 0);
    } catch { /* endpoint may be unavailable */ }
  }, []);

  const refreshBuilds = useCallback(async () => {
    try {
      const r = await api.builds();
      setBuilds(r.builds);
    } catch { /* keep the current list on error */ }
  }, []);

  const refreshProfiles = useCallback(async () => {
    try {
      const r = await api.profiles();
      setProfiles(r.profiles ?? []);
      setActiveProfile((cur) => {
        const next = r.active || r.profiles?.[0]?.id || "";
        // Preserve a still-valid current selection (e.g. user-switched profile).
        if (cur && r.profiles?.some((p) => p.id === cur)) return cur;
        return next;
      });
    } catch { /* endpoint may be unavailable */ }
  }, []);

  // Boot: wait for backend, load builds + status.
  useEffect(() => {
    (async () => {
      const ok = await waitForBackend();
      setReady(ok);
      if (!ok) { addLog("Could not reach the backend service.", "error"); return; }
      try {
        const [b, s, c] = await Promise.all([api.builds(), api.status(), api.credentials()]);
        setBuilds(b.builds);
        setStatus(s);
        setUsername(c.username);
      } catch (e: any) {
        addLog(`Startup error: ${e?.message}`, "error");
      }
      await refreshProfiles();
      api.updateCheck()
        .then((u) => {
          if (u.available) addLog(`Update available: v${u.latest_version} - see Settings.`, "info");
        })
        .catch(() => {});
      // A fresh install gets the first-run setup instead of the release notes.
      const setup = await api.setupState().catch(() => ({ show: false }));
      if (setup.show) { setShowSetup(true); return; }
      // Show the "What's New" dialog once per version (backend tracks "seen").
      api.whatsNew()
        .then((w) => {
          if (w.show && w.notes) setWhatsNew({ version: w.version, notes: w.notes });
        })
        .catch(() => { /* gracefully skip if unavailable */ });
    })();
  }, [addLog, refreshProfiles]);

  const finishSetup = useCallback(() => {
    setShowSetup(false);
    api.setupDone().catch(() => {});
    refreshStatus();
  }, [refreshStatus]);

  const closeWhatsNew = useCallback(() => {
    setWhatsNew(null);
    api.whatsNewSeen().catch(() => {});
  }, []);

  // Refresh conflict count whenever the active profile changes.
  useEffect(() => {
    if (activeProfile) refreshConflicts();
  }, [activeProfile, refreshConflicts]);

  // WebSocket event stream.
  useEffect(() => {
    if (!ready) return;
    const disconnect = connectEvents(
      (e: WsEvent) => handleEvent(e),
      () => {},
      () => {}
    );
    return disconnect;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready]);

  const handleEvent = (e: WsEvent) => {
    switch (e.type) {
      case "auth":
        if (e.logged_in) { setUsername(e.username); refreshStatus(); }
        break;
      case "log":
        addLog(e.message, e.tag);
        break;
      case "nxm_wait":
        setNxmHandler(e.active);
        break;
      case "status": {
        if (e.status === "WAITING_PATCHER") setPatcherMod(e.file_id);
        else if (FINAL.has(e.status) && patcherRef.current === e.file_id) setPatcherMod(null);
        setRuntime((prev) => ({
          ...prev,
          [e.file_id]: {
            ...(prev[e.file_id] ?? DEFAULT_RUNTIME),
            status: e.status,
            detail: e.detail,
            // Keep the full failure reason around so it can be reviewed later.
            ...(e.status === "ERROR" ? { error: e.detail } : {}),
            ...(FINAL.has(e.status) ? { progress: e.status === "DONE" ? 100 : prev[e.file_id]?.progress ?? 0 } : {}),
          },
        }));
        break;
      }
      case "progress": {
        const label = e.total_kb ? `${fmtKb(e.kb)} / ${fmtKb(e.total_kb)}` : `${fmtKb(e.kb)}`;
        setRuntime((prev) => ({
          ...prev,
          [e.file_id]: {
            ...(prev[e.file_id] ?? DEFAULT_RUNTIME), progress: e.pct * 100, progressLabel: label,
            // Bytes are arriving, so any "waiting for you" note is out of date.
            ...(prev[e.file_id]?.status === "DOWNLOADING" && e.kb > 0 ? { detail: "" } : {}),
          },
        }));
        break;
      }
      case "install_progress":
        setRuntime((prev) => ({
          ...prev,
          [e.file_id]: { ...(prev[e.file_id] ?? DEFAULT_RUNTIME), progress: e.pct * 100, progressLabel: e.label },
        }));
        break;
      case "confirm":
        setConfirmReq({ id: e.id, title: e.title, body: e.body, options: e.options });
        break;
      case "manual":
        setRuntime((prev) => ({
          ...prev,
          [e.file_id]: {
            ...(prev[e.file_id] ?? DEFAULT_RUNTIME),
            status: "MANUAL", detail: e.folder, progress: 100,
            manualFolder: e.folder, manualReadme: e.readme,
          },
        }));
        break;
      case "library":
        if (e.event === "import_folder_done") {
          addLog(`Imported ${e.count} mod(s) from folder.`, "success");
        }
        if (e.event === "bulk_progress") {
          setBulkProgress(e.done
            ? null
            : { action: e.action, current: e.current, total: e.total, mod: e.mod });
          // A progress tick is not a data change; refreshing per mod would
          // refetch the library hundreds of times during one removal.
          break;
        }
        // Refresh conflicts and data on any library event (import, toggle, delete).
        refreshConflicts();
        setDataTick((t) => t + 1);
        break;
      case "pipeline":
        if (e.event === "started") { setRunning(true); setPaused(false); }
        else if (e.event === "finished") {
          setRunning(false); setPaused(false); setPatcherMod(null);
          addLog(
            `Finished: ${e.done}/${e.total} installed` +
              (e.manual ? `, ${e.manual} need a manual step` : "") +
              (e.errors ? `, ${e.errors} error(s)` : "") + ".",
            e.errors ? "warning" : e.manual ? "info" : "success");
          refreshConflicts();
          setDataTick((t) => t + 1);
        }
        break;
    }
  };

  // Keep a ref of patcherMod for use inside the event handler closure.
  const patcherRef = useRef<string | null>(null);
  useEffect(() => { patcherRef.current = patcherMod; }, [patcherMod]);

  // Overall progress = finished / total. MANUAL counts as finished (the app has
  // done all it can; the player just has a few hand steps left) but is tracked
  // separately so it never reads as a failure. Mods installed in an earlier run
  // are not part of this run, so they have no status but are finished too.
  const { done, errors, manual, overall } = useMemo(() => {
    let d = 0, er = 0, mn = 0;
    for (const m of mods) {
      const st = runtime[m.file_id]?.status;
      if (st === "DONE" || st === "SKIPPED") d++;
      else if (m.installed && (!st || st === "PENDING")) d++;
      else if (st === "MANUAL") { d++; mn++; }
      else if (st === "ERROR") { d++; er++; }
    }
    return { done: d, errors: er, manual: mn, overall: mods.length ? (d / mods.length) * 100 : 0 };
  }, [mods, runtime]);

  // Player marked a manual mod as finished by hand - reflect it as done.
  const markManualDone = useCallback((fileId: string) => {
    setRuntime((prev) => ({
      ...prev,
      [fileId]: { ...(prev[fileId] ?? DEFAULT_RUNTIME), status: "DONE", progress: 100 },
    }));
  }, []);

  const handleSignOut = useCallback(() => {
    api.logout().catch(() => {});
    setUsername("");
    setStatus((prev) => (prev ? { ...prev, logged_in: false } : prev));
    addLog("Signed out.", "muted");
  }, [addLog]);

  return (
    <AppShell
      active={view}
      onNavigate={setView}
      status={status}
      username={username}
      running={running}
      overallPct={overall}
      conflictCount={conflictCount}
      onSignIn={() => setShowLogin(true)}
      onSignOut={handleSignOut}
      onOpenAccount={() => openSettingsSection("account")}
    >
      {visited.has("builds") && <div hidden={view !== "builds"} className="h-full">
        <BuildsView
          active={view === "builds"}
          ready={ready}
          loggedIn={!!status?.logged_in}
          builds={builds}
          refreshBuilds={refreshBuilds}
          selectedBuild={selectedBuild}
          onSelectBuild={setSelectedBuild}
          mods={mods}
          setMods={setMods}
          runtime={runtime}
          resetRuntime={resetRuntime}
          running={running}
          paused={paused}
          overall={overall}
          done={done}
          errors={errors}
          manual={manual}
          markManualDone={markManualDone}
          patcherMod={patcherMod}
          clearPatcher={() => setPatcherMod(null)}
          addLog={addLog}
          setRunning={setRunning}
          setPaused={setPaused}
          requestLogin={() => setShowLogin(true)}
          onSetupNexus={() => openSettingsSection("account")}
          activeProfile={activeProfile}
        />
      </div>}
      {visited.has("library") && <div hidden={view !== "library"} className="h-full">
        <LibraryView
          onGoToBuilds={() => setView("builds")}
          onGoToConflicts={() => setView("conflicts")}
          addLog={addLog}
          refreshTick={dataTick}
          profiles={profiles}
          activeProfile={activeProfile}
          setActiveProfile={setActiveProfile}
          bulkProgress={bulkProgress}
        />
      </div>}
      {visited.has("conflicts") && <div hidden={view !== "conflicts"} className="h-full">
        <ConflictsView
          refreshTick={dataTick}
          profiles={profiles}
          activeProfile={activeProfile}
          setActiveProfile={setActiveProfile}
          addLog={addLog}
          onResolved={() => { refreshConflicts(); setDataTick((t) => t + 1); }}
          onCountChange={setConflictCount}
        />
      </div>}
      {visited.has("activity") && <div hidden={view !== "activity"} className="h-full">
        <ActivityView logs={logs} onClear={() => setLogs([])} />
      </div>}
      {visited.has("settings") && <div hidden={view !== "settings"} className="h-full">
        <SettingsView
          status={status}
          username={username}
          onSignIn={() => setShowLogin(true)}
          onSignOut={handleSignOut}
          addLog={addLog}
          profiles={profiles}
          activeProfile={activeProfile}
          setActiveProfile={setActiveProfile}
          refreshProfiles={refreshProfiles}
          section={settingsSection}
          setSection={setSettingsSection}
          onRunSetup={() => setShowSetup(true)}
        />
      </div>}

      <LoginDialog
        open={showLogin}
        onClose={() => setShowLogin(false)}
        onLoggedIn={(u) => { setUsername(u); refreshStatus(); }}
      />

      <SetupWizard
        open={showSetup}
        onFinish={finishSetup}
        loggedIn={!!status?.logged_in}
        username={username}
        onLoggedIn={(u) => { setUsername(u); refreshStatus(); }}
        profiles={profiles}
        refreshProfiles={refreshProfiles}
        addLog={addLog}
      />

      <ConfirmDialog
        request={confirmReq}
        onAnswer={(id, choice) => {
          setConfirmReq(null);
          api.answerConfirm(id, choice).catch(() => {});
        }}
      />

      <WhatsNew
        open={!!whatsNew}
        version={whatsNew?.version ?? ""}
        notes={whatsNew?.notes ?? ""}
        onClose={closeWhatsNew}
      />
    </AppShell>
  );
}

function fmtKb(kb: number): string {
  if (kb >= 1024) return `${(kb / 1024).toFixed(1)} MB`;
  return `${kb} KB`;
}
