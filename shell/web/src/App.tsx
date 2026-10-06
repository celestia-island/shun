import { computed, defineComponent, onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  CheckCircle2,
  Moon,
  ChevronLeft,
  ChevronRight,
  Sun,
  XCircle,
} from "lucide-vue-next";
import {
  HkAlert,
  HkButton,
  HkCheckbox,
  HkProgressBar,
  HkScrollContainer,
  HkSelect,
  HkTimeline,
} from "@celestia-island/hikari";
import HkWallpaperBackdrop from "@celestia-island/hikari/components/HkWallpaperBackdrop";
import {
  initWallpaper,
  useWallpaper,
} from "@celestia-island/hikari/theme/useWallpaper";

import { composeAgreementDocs } from "./agreementDoc";
import AppTitleBar from "./components/AppTitleBar";
import PathField, { type DriveInfo } from "./components/PathField";
import LogPane, { type LogLine } from "./components/LogPane";
import PairingPane from "./components/PairingPane";
import {
  isInstallerLocale,
  LOCALE_OPTIONS,
  resolveSystemLocale,
  strings,
  type InstallerLocale,
  type InstallerStrings,
} from "./i18n";
import { renderRichText } from "./richText";
import { invoke, listen, openDirectory, tauriWindow } from "./tauri";

/**
 * Installer shell UI — a step-driven delivery wizard rendered with hikari
 * components: a left step rail (language → location → license → install →
 * done), centered panes, the backend-resolved license agreement, and
 * done-page shortcut toggles plus an optional immediate launch, all
 * applied only when the final confirmation runs (nothing is created
 * during the install itself). The wizard opens on the language step,
 * whose picker alone decides the locale for every later pane. An install
 * failure lands on the done step as a failure variant with retry/close
 * actions — nothing returns to earlier steps once the install started.
 * The license step pages through the localized documents the backend
 * resolves at build time, rendered as restricted rich text; agreeing
 * covers all of them. Evernight installs per-user only — there is no
 * portable/USB delivery mode.
 *
 * When the shell runs as the uninstaller (`/uninstall`, probed via
 * `is_uninstall_mode`), the wizard layout is replaced by a standalone
 * centered uninstall page: confirm (卸载, or 修复安装 which re-runs the
 * local delivery over the existing install dir) → indeterminate progress
 * → done/fail.
 */

type Mode = "local";
type StepKey = "language" | "mode" | "pairing" | "license" | "install" | "done" | `content:${string}`;

interface DirCandidate {
  kind: string;
  path: string;
  writable: boolean;
}

interface DirDefaults {
  dir: string;
  removable: boolean;
  candidates: DirCandidate[];
}

interface LicenseDoc {
  title: string;
  body: string;
}

interface FlowEventPayload {
  phase?: string;
  step?: string;
  percent?: number | null;
  message?: string;
  record?: {
    // shun tags FlowLog records with a `log` discriminator (serde tag),
    // not `type` — kebab-case values: file-write / file-reuse / warning /
    // script-begin / script-line / command-done.
    log?: string;
    path?: string;
    name?: string;
    line?: string;
    command?: string;
    code?: string;
    detail?: string;
  };
}

// Quick-candidate row: label + glyph per candidate kind; drive candidates
// show the path itself (a row of drive roots reads better than a bare
// "磁盘"). The two product nouns are locale-independent.

// Step keys in rail order (language leads, the wizard's first step); the
// labels resolve from the string table per render so a locale switch
// relabels the timeline live.
const BASE_STEP_KEYS = ["language", "mode", "pairing", "license", "install", "done"] as const;

/** One flow entry: the pane key + its timeline label. */
interface FlowStep {
  key: StepKey;
  label: string;
  kind: string;
  /** The resolved pipeline step backing a `content:` pane. */
  step?: { kind: string; title: string; body?: string | null };
  /** The pairing contract backing a `pairing` pane. */
  pairing?: {
    source: { kind: string; official?: string; allow_custom?: boolean };
    identity?: { node_id?: boolean; name?: boolean; tier?: number };
    env_file?: string;
  };
}

/** The wizard's flow from the resolved pipeline: content steps slot in
 * at their DECLARATION position relative to the license (steps render
 * in declaration order); mode/scope fold into the location pane and
 * install/done cap the array. */
function buildFlow(
  steps: {
    kind: string;
    title: string;
    body?: string | null;
    pairing?: FlowStep["pairing"];
  }[] | null,
  labels: {
    language: string;
    mode: string;
    pairing: string;
    license: string;
    install: string;
    done: string;
  },
): FlowStep[] {
  const out: FlowStep[] = [
    { key: "language", label: labels.language, kind: "language" },
    { key: "mode", label: labels.mode, kind: "mode" },
  ];
  let license = false;
  for (const st of steps ?? []) {
    if (st.kind === "content") {
      out.push({ key: `content:${out.length}`, label: st.title, kind: "content", step: st });
    } else if (st.kind === "license") {
      out.push({ key: "license", label: labels.license, kind: "license" });
      license = true;
    } else if (st.kind === "pairing" && st.pairing) {
      out.push({
        key: "pairing",
        label: labels.pairing,
        kind: "pairing",
        pairing: st.pairing,
      });
    }
  }
  if (!license) out.splice(2, 0, { key: "license", label: labels.license, kind: "license" });
  out.push({ key: "install", label: labels.install, kind: "install" });
  out.push({ key: "done", label: labels.done, kind: "done" });
  return out;
}

export default defineComponent({
  name: "InstallerApp",
  setup() {
    // `?step=` preview hook (static previews / dev); production passes no
    // query and starts at the language pane.
    const initialStep = (new URLSearchParams(window.location.search).get(
      "step",
    ) ?? "") as StepKey;
    const step = ref<StepKey>(
      (BASE_STEP_KEYS as readonly string[]).includes(initialStep as never) ? (initialStep as StepKey) : "language",
    );
    const mode = ref<Mode>("local");
    // Wizard locale — resolved synchronously from the system so first paint
    // (and the ?step= previews) always has strings, then overridden by the
    // saved preference once the backend answers (saved > system). A failed
    // invoke (e.g. non-Tauri preview) keeps the system resolution.
    const locale = ref<InstallerLocale>(
      resolveSystemLocale(navigator.language),
    );
    const dir = ref("");
    // The hint under the path field: a semantic kind resolved to text at
    // render time (so a locale switch relabels it), or a raw backend error
    // message that passes through as-is.
    const hintKind = ref<"local" | "error">("local");
    const hintError = ref("");
    const drives = ref<DriveInfo[]>([]);
    const candidates = ref<DirCandidate[]>([]);
    // Optional attachments (get_config): the checkbox row per component
    // that is NOT bundled into this build — bundled ones list as
    // included, the checked ones stream in right after the install.
    // The resolved wizard pipeline (get_config) — content steps render
    // from here at their declaration position.
    const stepsCfg = ref<{
      kind: string;
      title: string;
      body?: string | null;
      pairing?: FlowStep["pairing"];
    }[]>([]);
    const flashDeclared = ref(false);
    // Whether the pairing pane reached its success card (unlocks Next).
    const pairingClaimed = ref(false);
    const attachments = ref<
      { key: string; title: string; included: boolean; size: number | null }[]
    >([]);
    const attachmentPicked = ref<Record<string, boolean>>({});
    // Live writability of the shown path: null while the probe is in
    // flight or the box is empty, true/false once the backend answered.
    const dirWritable = ref<boolean | null>(null);
    const licenseDocs = ref<LicenseDoc[]>([]);
    const licenseIndex = ref(0);
    // The displayed agreement pages: the backend's copyright notice opens a
    // merged first document (the FOSS notice and the short telemetry
    // disclosure ride along as markdown sections); a locale switch
    // recomposes it live.
    const agreementDocs = computed(() =>
      composeAgreementDocs(licenseDocs.value, locale.value),
    );
    const agreed = ref(false);
    // License-step notice countdown: holds the agree button for five
    // seconds on EVERY license-step entry so the merged free & open-source
    // sections opening the first agreement page cannot be skipped unseen.
    const noticeCountdown = ref(5);
    const desktopShortcut = ref(true);
    const startMenuShortcut = ref(true);
    // Done-page option: start the installed app (portable copies launch
    // the portable copy) right when the confirmation closes the wizard.
    const launchAfterInstall = ref(true);
    const overall = ref<number | null>(null);
    const flowStep = ref("");
    const installFailed = ref(false);
    const failMessage = ref("");
    const note = ref<{ text: string; kind: "ok" | "err" } | null>(null);

    // Uninstall mode (`/uninstall` without --silent): replaces the whole
    // wizard layout with a standalone confirm → progress → done page.
    // Null while the probe is in flight — nothing renders until it lands,
    // so the uninstaller never flashes the wizard it will not run.
    const uninstallMode = ref<boolean | null>(null);
    // 卸载 drives running → done/failed; 修复安装 drives the parallel
    // repairing → repaired/repair_failed triple (same running view).
    const uninstallPhase = ref<
      "idle" | "running" | "done" | "failed" | "repairing" | "repaired" | "repair_failed"
    >("idle");
    const uninstallError = ref("");

    // Install log pane: structured events composed into localized lines
    // with HH:MM:SS stamps; ordering follows the manifest (newest-first
    // default) with a per-run toggle. Line text resolves from the picked
    // wizard locale (`install.progress` in i18n.ts); unknown backend verbs
    // pass through verbatim (they are composed English).
    const logLines = ref<LogLine[]>([]);
    const logOrder = ref<"newest" | "oldest">("newest");
    // The log pane folds into a one-line drawer by default; error records
    // force it open so the cause is visible without a manual click.
    const logExpanded = ref(false);
    const stamp = () => new Date().toTimeString().slice(0, 8);
    const pushLog = (kind: LogLine["kind"], text: string) => {
      logLines.value.push({ time: stamp(), kind, text });
      if (logLines.value.length > 500) logLines.value.shift();
      if (kind === "error") logExpanded.value = true;
    };
    // The flow's progress labels are composed English verbs; render the
    // ones we know in the wizard language and pass the rest through.
    const localizeStep = (step: string): string => {
      const progress = withProduct(strings(locale.value)).install.progress;
      // Installer-shell steps beyond the payload verbs (main.rs): the
      // pre-kill notice and the stale-file cleanup summary.
      if (step === "Stopping evernight.exe") return progress.stopApp;
      const stale = /^Removed (\d+) stale file/.exec(step);
      if (stale) return progress.removedStale(Number(stale[1]));
      const m = /^(Extracting|Reusing|Downloading|Registering|Writing)\s+(.+)$/.exec(step);
      if (!m) return step;
      const verb = progress.verbs[m[1]];
      return verb ? `${verb} ${m[2]}` : step;
    };
    const phaseLabel = (phase: string | undefined): string => {
      const phases = strings(locale.value).install.progress.phases;
      switch (phase) {
        case "download": return phases.download;
        case "extract": return phases.extract;
        case "register": return phases.register;
        default: return phases.fallback;
      }
    };


    const running = ref(false);
    // True while the done-page confirmation is applying the shortcut
    // choices — the finish button stays disabled for that window.
    const finishing = ref(false);

    async function refreshDefaults() {
      const defaults = await invoke<DirDefaults>("default_dir", { mode: mode.value });
      dir.value = defaults.dir;
      candidates.value = defaults.candidates;
      hintKind.value = "local";
      hintError.value = "";
    }

    const identity = ref<{ version: string; flavor: string } | null>(null);
    // The manifest's product logo as a data URL — the same embedded
    // bytes the egui face renders; falls back to the stock placeholder
    // when the manifest ships no logo (or the probe fails).
    const logoUrl = ref<string>("/logo.webp");
    // Theme backgrounds from the delivery manifest: the page/rail/pane
    // CSS layers (solid, gradient, wallpaper data URLs) — the rail sits
    // at a slight brightness offset from the pane unless explicitly
    // overridden (the classic installer split).
    const theme = ref<{
      background?: { css: string } | null;
      railBackground?: { css: string } | null;
      paneBackground?: { css: string } | null;
      mode?: "system" | "light" | "dark" | null;
      // ThemeConfig serializes kebab-case; the toggle rides this flag.
      "user-adjustable"?: boolean | null;
      wallpaper?: {
        sources: Array<
          { video: string } | { image: string } | { pipeline: string }
        >;
      } | null;
    } | null>(null);
    // Product identity from the delivery manifest — the display name
    // every %PRODUCT% token in the string table resolves to.
    const product = ref("");
    // Deep-map the string table, substituting %PRODUCT% — functions
    // (the agreeInstall countdown label) and non-strings pass through
    // untouched; a JSON round-trip would silently drop them and crash
    // the license step's render.
    const substitute = (value: unknown): unknown => {
      if (typeof value === "string") {
        return value.replaceAll("%PRODUCT%", product.value || "");
      }
      if (Array.isArray(value)) return value.map(substitute);
      if (value && typeof value === "object") {
        return Object.fromEntries(
          Object.entries(value).map(([k, v]) => [k, substitute(v)]),
        );
      }
      return value;
    };
    const withProduct = (s: InstallerStrings): InstallerStrings =>
      substitute(s) as InstallerStrings;

    // The first candidate the install would actually accept — the row
    // highlights it while the current path fails the live probe.
    const firstWritableCandidate = computed(
      () => candidates.value.find((candidate) => candidate.writable) ?? null,
    );

    // Live writability probe, debounced so typing does not hammer the
    // backend (the probe creates + deletes a temp file per call). The
    // answer only lands while it is still about the current path.
    let writableTimer: ReturnType<typeof setTimeout> | null = null;
    // Re-entering the pairing pane re-arms its Next gate: a later FAILED
    // re-pair must not ride a stale unlocked button.
    watch(step, (value) => {
      if (value === "pairing") pairingClaimed.value = false;
    });
    watch(dir, (value) => {
      const target = value.trim();
      if (writableTimer !== null) clearTimeout(writableTimer);
      if (!target) {
        dirWritable.value = null;
        return;
      }
      dirWritable.value = null;
      writableTimer = setTimeout(() => {
        invoke<boolean>("check_dir_writable", { dir: target })
          .then((ok) => {
            if (dir.value.trim() === target) dirWritable.value = ok;
          })
          .catch(() => {
            if (dir.value.trim() === target) dirWritable.value = null;
          });
      }, 400);
    });

    // The license documents are backend-resolved per wizard locale
    // (build-time artifacts; every offered locale has a dedicated set,
    // and anything else still lands the English fallback). A locale
    // switch re-fetches; a response from a superseded request is dropped
    // so a slow earlier locale can never win.
    function refreshLicenseDocs() {
      const requested = locale.value;
      invoke<LicenseDoc[]>("get_license_docs", { locale: requested })
        .then((docs) => {
          if (locale.value !== requested) return;
          licenseDocs.value = docs;
          licenseIndex.value = 0;
        })
        .catch(() => {});
    }
    watch(locale, refreshLicenseDocs);

    // The hikari wallpaper stack: register the manifest's candidate
    // chain in order and activate the first; a source that cannot render
    // stands the surface down to the solid floor, which advances the
    // chain to the next candidate (online mirrors, then the embedded
    // fallback) — the priority contract.
    const wallpaperIds: string[] = [];
    let wallpaperCursor = 0;
    function bootWallpaper() {
      const sources = theme.value?.wallpaper?.sources ?? [];
      if (sources.length === 0) return;
      initWallpaper();
      const wallpaper = useWallpaper();
      const addCustomWallpaper = wallpaper.addCustomWallpaper;
      const setActiveWallpaper = wallpaper.setActiveWallpaper;
      for (const [index, source] of sources.entries()) {
        const hikariSource =
          "video" in source
            ? { type: "video", url: source.video }
            : "image" in source
              ? { type: "image", url: source.image }
              : { type: "pipeline", preset: source.pipeline };
        wallpaperIds.push(
          addCustomWallpaper(`shun-theme-${index}`, hikariSource as never),
        );
      }
      setActiveWallpaper(wallpaperIds[0]);
      // Advance on stand-down: when the active candidate renders as the
      // solid floor (load failure — hikari paints nothing and says
      // nothing), try the next one.
      watch(
        () => useWallpaper().wallpaperType.value,
        (kind) => {
          if (
            kind === "solid" &&
            wallpaperCursor < wallpaperIds.length - 1 &&
            useWallpaper().activeWallpaperId.value === wallpaperIds[wallpaperCursor]
          ) {
            wallpaperCursor += 1;
            setActiveWallpaper(wallpaperIds[wallpaperCursor]);
          }
        },
      );
    }

    onMounted(() => {
      invoke<string | null>("get_saved_language")
        .then((saved) => {
          // Saved preference wins over the system resolution; anything the
          // wizard does not offer is ignored.
          if (isInstallerLocale(saved)) locale.value = saved;
        })
        .catch(() => {});
      invoke<boolean>("is_uninstall_mode")
        .then((flag) => {
          uninstallMode.value = flag;
        })
        // A failed probe (backend not ready, non-Tauri preview) renders
        // the wizard: defaulting to "uninstall" would strand a normal
        // install, and null strands EVERYTHING behind the in-flight
        // guard — the blank-window failure mode.
        .catch(() => {
          uninstallMode.value = false;
        });
      refreshDefaults().catch((err) => { hintKind.value = "error"; hintError.value = String(err); });
      invoke<{ version: string; flavor: string }>("get_identity")
        .then((id) => { identity.value = id; })
        .catch(() => {});
      invoke<{ kind: string; data: string } | null>("get_logo")
        .then((logo) => {
          if (logo) logoUrl.value = `data:image/${logo.kind};base64,${logo.data}`;
        })
        .catch(() => {});
      invoke<{ product: { name: string }; flash?: boolean } & { attachments?: { key: string; title: string; included: boolean; size: number | null }[] }>("get_config")
        .then((view) => {
          product.value = view.product?.name ?? "";
          const viewSteps = (view as { steps?: { kind: string; title: string; body?: string | null }[] }).steps ?? [];
          stepsCfg.value = viewSteps;
          flashDeclared.value = Boolean(view.flash);
          attachments.value = view.attachments ?? [];
          attachmentPicked.value = Object.fromEntries(
            attachments.value.map((a) => [a.key, true]),
          );
          theme.value = (view as { theme?: unknown }).theme as typeof theme.value;
          applyThemeMode();
          bootWallpaper();
          // The OS theme clock: an OS light/dark flip re-applies
          // while the mode is unpinned, so the wizard follows live.
          prefersDark?.addEventListener?.("change", () => {
            if (userPinned.value === null) applyThemeMode();
          });
        })
        .catch(() => {});
      invoke<DriveInfo[]>("list_drives")
        .then((list) => { drives.value = list; })
        .catch(() => {});
      invoke<LicenseDoc[]>("get_license_docs", { locale: locale.value })
        .then((docs) => {
          licenseDocs.value = docs;
          licenseIndex.value = 0;
        })
        .catch(() => {});
      invoke<{ log_level: string; log_order: string }>("get_shell_prefs")
        .then((prefs) => {
          logOrder.value = prefs.log_order === "oldest" ? "oldest" : "newest";
        })
        .catch(() => {});
      listen<FlowEventPayload>("install-progress", (event) => {
        // Structured log records compose into localized pane lines, keyed
        // to the picked wizard locale.
        if (event.record) {
          const r = event.record;
          const kind = r.log;
          const progress = withProduct(strings(locale.value)).install.progress;
          if (kind === "file-write" && r.path) {
            pushLog("echo", progress.writing(r.path));
          } else if (kind === "file-reuse" && r.path) {
            pushLog("echo", progress.reusing(r.path));
          } else if (kind === "warning") {
            const text = [r.code, r.detail].filter(Boolean).join(": ");
            if (text) pushLog("error", text);
          } else if (kind === "script-begin" && r.name) {
            pushLog("step", progress.runningScript(r.name));
          } else if (kind === "script-line" && r.line) {
            pushLog("echo", r.line);
          } else if (kind === "command-done" && r.command) {
            pushLog("ok", `✓ ${r.command}`);
          }
        }
        if (event.phase) flowStep.value = localizeStep(event.step ?? "") || phaseLabel(event.phase);
        if (event.percent != null) overall.value = Math.round(event.percent);
        if (event.message) {
          installFailed.value = true;
          failMessage.value = event.message;
          pushLog("error", event.message);
        }
      });
      // Preview hook landed directly on the license step: arm the notice
      // countdown here too, not only on the wizard's go("license").
      if (step.value === "license") {
        licenseIndex.value = 0;
        startNoticeCountdown();
      }
    });

    onBeforeUnmount(() => {
      if (noticeTimer !== null) clearInterval(noticeTimer);
    });

    let noticeTimer: ReturnType<typeof setInterval> | null = null;

    // (Re)arm the notice countdown: clears any pending interval, resets to
    // 5, ticks down once a second, and clears itself when it reaches 0.
    function startNoticeCountdown() {
      if (noticeTimer !== null) clearInterval(noticeTimer);
      noticeCountdown.value = 5;
      noticeTimer = setInterval(() => {
        noticeCountdown.value -= 1;
        if (noticeCountdown.value <= 0) {
          noticeCountdown.value = 0;
          if (noticeTimer !== null) clearInterval(noticeTimer);
          noticeTimer = null;
        }
      }, 1000);
    }

    function go(next: StepKey) {
      step.value = next;
      if (next === "license") {
        licenseIndex.value = 0;
        startNoticeCountdown();
      }
      if (next === "install") {
        running.value = true;
        installFailed.value = false;
        failMessage.value = "";
        overall.value = null;
        flowStep.value = strings(locale.value).install.preparing;
        logLines.value = [];
        pushLog("step", strings(locale.value).install.startedLog);
      }
    }

    // Picker change: the ref alone drives every label on the next render;
    // the choice is persisted once the install starts (start()), when the
    // mode — and with it the portable write-skip — is finally known.
    function changeLocale(value: string) {
      if (!isInstallerLocale(value)) return;
      locale.value = value;
    }

    /** 裸盘符根目录（如选中的 D:\）不直接接收载荷：shun 0.3 的根盘
        保护会在其下自动垫一层文件夹（默认取产品名 Evernight），并让路径
        框始终显示真实目标。 */
    async function applyNestRootDir(raw: string) {
      const nested = await invoke<string>("nest_root_dir", { dir: raw });
      if (nested !== raw.trim()) showNote(strings(locale.value).target.nestedNote);
      dir.value = nested;
    }

    async function browse() {
      if (step.value !== "mode") return;
      const picked = await openDirectory(strings(locale.value).target.dialogTitle);
      if (picked) await applyNestRootDir(picked);
    }

    async function start() {
      // 手动输入的裸盘根目录先垫好文件夹再开跑——完成页与后续的
      // 快捷方式 / 启动命令用的都是改写后的真实路径。
      await applyNestRootDir(dir.value).catch(() => {});
      // 语言偏好在这里（而非选择器切换时）落盘：此刻安装方式已定，
      // portable (USB) 运行会跳过写入，不在宿主机上留下状态。
      void invoke("save_language", {
        language: locale.value,
        portable: false,
      }).catch(() => {});
      go("install");
      try {
        await invoke("start_install", {
          mode: mode.value,
          dir: dir.value.trim(),
          language: locale.value,
        });
        // Optional components the user picked (not bundled in this
        // build) stream in now — their progress lands in the log pane
        // through the same install-progress channel.
        for (const a of attachments.value) {
          if (a.included || !attachmentPicked.value[a.key]) continue;
          await invoke("download_attachment", {
            key: a.key,
            dir: dir.value.trim(),
          });
        }
        // No shortcut work here: the install creates none, and the done
        // pane's toggles take effect only on the final confirmation.
        overall.value = 100;
        step.value = "done";
      } catch (err) {
        installFailed.value = true;
        failMessage.value = String(err);
        // Failure lands on the done step too: the log lines stay (the
        // drawer auto-expanded on the error record) so the failure trail
        // remains readable next to the retry action. A retry resets them
        // in go("install").
        step.value = "done";
      } finally {
        running.value = false;
      }
    }

    // The done-page confirmation: applies the shortcut choices in one
    // shot for local installs (portable copies have none), optionally
    // starts the freshly installed app per the 立即启动 checkbox (for
    // portable it launches the portable copy), then closes the window.
    async function finish() {
      if (finishing.value) return;
      finishing.value = true;
      try {
        // The pairing claim rides the finish into the install dir as the
        // manifest's env-file (no-op when the run never paired).
        await invoke("write_pairing_env", { dir: dir.value.trim() });
        await invoke("set_shortcuts", {
          desktop: desktopShortcut.value,
          menu: startMenuShortcut.value,
          dir: dir.value.trim(),
        });
        if (launchAfterInstall.value) {
          await invoke("launch_app", { dir: dir.value.trim() });
        }
        tauriWindow()?.close();
      } catch (err) {
        showNote(String(err), "err");
      } finally {
        finishing.value = false;
      }
    }

    function toggleDesktop(v: boolean) {
      desktopShortcut.value = v;
    }

    function toggleMenu(v: boolean) {
      startMenuShortcut.value = v;
    }

    // The uninstall page's only action: run the shun uninstall (the
    // backend deletes the install dir this uninstaller sits in), then
    // flip to the done / failed view. shun emits no progress events, so
    // the running view is an indeterminate bar.
    async function runUninstall() {
      if (uninstallPhase.value !== "idle") return;
      uninstallPhase.value = "running";
      try {
        await invoke("perform_uninstall");
        uninstallPhase.value = "done";
      } catch (err) {
        uninstallError.value = String(err);
        uninstallPhase.value = "failed";
      }
    }

    // The uninstall page's repair action: re-runs the local delivery flow
    // over the install dir this uninstaller sits in (repairs damaged or
    // missing files; user data is preserved). The running view is the
    // same indeterminate bar as the uninstall itself — shun emits no
    // progress events the page surfaces here.
    async function runRepair() {
      if (uninstallPhase.value !== "idle") return;
      uninstallPhase.value = "repairing";
      try {
        const installDir = await invoke<string>("current_install_dir");
        await invoke("start_install", { mode: "local", dir: installDir });
        uninstallPhase.value = "repaired";
      } catch (err) {
        uninstallError.value = String(err);
        uninstallPhase.value = "repair_failed";
      }
    }

    function closeWindow() {
      tauriWindow()?.close();
    }

    function showNote(text: string, kind: "ok" | "err" = "ok") {
      note.value = { text, kind };
    }

    // Light/dark resolution (hikari rules, user direction): `system`
    // resolves from the SUN, not the OS preference — the same altitude
    // bands and first-paint estimate the egui face runs (day above
    // +6° → light, civil twilight/night → dark). Longitude is the
    // timezone offset's estimate; the clock re-checks every five
    // minutes so dawn and dusk flip a live wizard. `light`/`dark` pin
    // via [data-mode]; an explicit user toggle (when the manifest
    // allows it) wins for the session.
    // Light/dark resolution (hikari rules, user direction): `system`
    // follows the MACHINE's app theme — WebView2 honors Windows 11's
    // personalization light/dark through the prefers-color-scheme
    // query — not the sun. The query unavailable resolves LIGHT
    // (the documented floor). A live OS flip re-applies while the
    // wizard sits open; a user toggle (when the manifest allows it)
    // wins for the session.
    const systemWantsDark = (): boolean =>
      window.matchMedia?.("(prefers-color-scheme: dark)")?.matches ?? false;
    const prefersDark = window.matchMedia?.("(prefers-color-scheme: dark)");
    const themeMode = ref<"system" | "light" | "dark">("system");
    const userPinned = ref<"light" | "dark" | null>(null);
    const applyThemeMode = () => {
      const resolved =
        userPinned.value ?? theme.value?.mode ?? "system";
      // themeMode carries the EFFECTIVE side (system resolves against
      // the OS preference), so the caption toggle's sun/moon reflects
      // what the page actually shows instead of pinning on the raw
      // "system".
      const effective =
        resolved === "system"
          ? systemWantsDark()
            ? "dark"
            : "light"
          : resolved;
      themeMode.value = effective;
      const root = document.documentElement;
      root.dataset.mode =
        resolved === "system" ? "" : resolved;
    };
    const toggleTheme = () => {
      const dark =
        userPinned.value === "dark" ||
        (userPinned.value === null &&
          (theme.value?.mode === "dark" ||
            (theme.value?.mode ?? "system") === "system" &&
              window.matchMedia("(prefers-color-scheme: dark)").matches));
      userPinned.value = dark ? "light" : "dark";
      applyThemeMode();
    };

    // Theme CSS: page background, per-side rail/pane layers. Without a
    // rail override the rail keeps its default translucent offset over
    // the page background (the brightness split).
    const pageStyle = computed(() => ({
      background: theme.value?.background?.css ?? undefined,
    }));
    const railStyle = computed(() => ({
      background: theme.value?.railBackground?.css ?? undefined,
    }));
    const paneStyle = computed(() => ({
      background: theme.value?.paneBackground?.css ?? undefined,
    }));

    return () => {
      const s = withProduct(strings(locale.value));
      // Uninstall page: a standalone centered pane instead of the wizard
      // layout — no step rail, no install panes, no footer nav. While the
      // mode probe is still in flight, render nothing.
      if (uninstallMode.value === null) {
        return (
          <>
            <AppTitleBar
              icon={logoUrl.value}
              title={s.title}
              subtitle={identity.value ? `v${identity.value.version}` : ""}
              showMaximize={false}
            />
            <main class="installer" />
          </>
        );
      }
      if (uninstallMode.value) {
        const uninstallPane =
          uninstallPhase.value === "idle" ? (
            <section class="wizard-pane wizard-uninstall">
              <h1>{s.uninstall.heading}</h1>
              <p class="wizard-sub">{s.uninstall.sub}</p>
              <div class="wizard-uninstall__actions">
                <HkButton variant="ghost" onClick={closeWindow}>
                  {s.uninstall.cancel}
                </HkButton>
                <HkButton variant="ghost" onClick={runRepair}>
                  {s.uninstall.repair}
                </HkButton>
                <HkButton variant="danger" onClick={runUninstall}>
                  {s.uninstall.uninstall}
                </HkButton>
              </div>
            </section>
          ) : uninstallPhase.value === "running" || uninstallPhase.value === "repairing" ? (
            <section class="wizard-pane wizard-uninstall">
              <img src={logoUrl.value} alt="" class="wizard-logo" />
              <HkProgressBar status="loading" size="md" />
              <p class="wizard-step">
                {uninstallPhase.value === "repairing" ? s.uninstall.repairing : s.uninstall.uninstalling}
              </p>
            </section>
          ) : uninstallPhase.value === "done" || uninstallPhase.value === "repaired" ? (
            <section class="wizard-pane wizard-uninstall">
              <CheckCircle2
                size={56}
                color="rgb(var(--color-success))"
                stroke-width={1.5}
              />
              <p class="wizard-done__title">
                {uninstallPhase.value === "repaired" ? s.uninstall.doneRepair : s.uninstall.doneUninstall}
              </p>
              <div class="wizard-uninstall__actions">
                <HkButton variant="primary" onClick={closeWindow}>
                  {s.uninstall.close}
                </HkButton>
              </div>
            </section>
          ) : (
            <section class="wizard-pane wizard-uninstall">
              <XCircle
                size={56}
                color="rgb(var(--color-error))"
                stroke-width={1.5}
              />
              <p class="wizard-done__title wizard-done__title--fail">
                {uninstallPhase.value === "repair_failed" ? s.uninstall.failedRepair : s.uninstall.failedUninstall}
              </p>
              <p class="wizard-uninstall__error">{uninstallError.value}</p>
              <div class="wizard-uninstall__actions">
                <HkButton variant="primary" onClick={closeWindow}>
                  {s.uninstall.close}
                </HkButton>
              </div>
            </section>
          );

        return (
          <>
            <HkWallpaperBackdrop />
            <AppTitleBar icon={logoUrl.value} title={s.uninstallTitle} showMaximize={false} />
            <main class="installer" style={pageStyle.value}>
              <div class="wizard-layout__pane">{uninstallPane}</div>
            </main>
          </>
        );
      }

      const flow = buildFlow(stepsCfg.value, {
        ...withProduct(strings(locale.value)).steps,
        pairing: withProduct(strings(locale.value)).pairing.title,
      });
      const flowIndex = (key: string) => flow.findIndex((f) => f.key === key);
      const timelineSteps = flow.map((f) => ({ key: f.key, label: f.label }));

      const pane =
        step.value === "language" ? (
          <section class="wizard-pane wizard-language">
            <h1>{s.language.title}</h1>
            <p class="wizard-sub">{s.language.sub}</p>
            <div class="wizard-language__select">
              <HkSelect
                modelValue={locale.value}
                options={LOCALE_OPTIONS}
                onUpdate:modelValue={changeLocale}
              />
            </div>
          </section>
        ) : step.value === "mode" ? (
          <section class="wizard-pane">
            <h1>{s.mode.title}</h1>
            <p class="wizard-sub">{s.mode.sub}</p>

            <section class="wizard-target">
              <label class="wizard-target__label" for="dir-input">{s.target.label}</label>
              <PathField
                modelValue={dir.value}
                disabled={running.value}
                drives={drives.value}
                candidates={candidates.value}
                labels={s.pathField}
                {...{ "onPick-candidate": (path: string) => {
                  dir.value = path;
                  void applyNestRootDir(path).catch(() => {});
                } }}
                onUpdate:modelValue={(v: string) => (dir.value = v)}
                onBrowse={browse}
                onBlur={() => {
                  // 离开输入框即校平裸盘根目录，路径框保持真实目标。
                  if (step.value === "mode" && !running.value) {
                    void applyNestRootDir(dir.value).catch(() => {});
                  }
                }}
              />
              <p class="wizard-target__hint">
                {hintKind.value === "error" ? hintError.value : s.target.hintLocal}
              </p>
              {dirWritable.value === false && (
                <p class="wizard-target__warning">
                  {firstWritableCandidate.value
                    ? s.target.warnUnwritable
                    : s.target.warnNoWritable}
                </p>
              )}
              {identity.value && (
                <p class="wizard-identity">
                  {`${product.value} ${identity.value.version} · `}
                  {identity.value.flavor === "full-webview2"
                    ? s.flavors.fullWebview2
                    : identity.value.flavor === "full"
                      ? s.flavors.full
                      : identity.value.flavor}
                </p>
              )}
              {flashDeclared.value && (
                <p class="wizard-target__flash-hint">{s.target.flashHint}</p>
              )}
              {attachments.value.length > 0 && (
                <div class="wizard-attachments">
                  <p class="wizard-attachments__title">{s.target.attachTitle}</p>
                  {attachments.value.map((a) => (
                    <label key={a.key} class="wizard-attachments__row">
                      <input
                        type="checkbox"
                        checked={a.included || attachmentPicked.value[a.key]}
                        disabled={a.included}
                        onChange={(e) => {
                          attachmentPicked.value = {
                            ...attachmentPicked.value,
                            [a.key]: (e.target as HTMLInputElement).checked,
                          };
                        }}
                      />
                      <span class="wizard-attachments__name">{a.title}</span>
                      <span class="wizard-attachments__meta">
                        {a.included
                          ? s.target.attachBundled
                          : a.size
                            ? `${(a.size / 1048576).toFixed(1)} MB`
                            : ""}
                      </span>
                    </label>
                  ))}
                </div>
              )}
            </section>
          </section>
        ) : step.value.startsWith("content:") ? (
          (() => {
            const contentStep = flow[flowIndex(step.value)].step;
            return (
              <section class="wizard-pane">
                <h1>{contentStep?.title}</h1>
                <HkScrollContainer class="license-box" axis="vertical">
                  <div
                    class="license-doc"
                    innerHTML={renderRichText(contentStep?.body ?? "")}
                  />
                </HkScrollContainer>
              </section>
            );
          })()
        ) : step.value === "pairing" && flow[flowIndex(step.value)]?.pairing ? (
          <PairingPane
            strings={withProduct(strings(locale.value)).pairing}
            config={flow[flowIndex(step.value)].pairing!}
            onClaimed={() => (pairingClaimed.value = true)}
          />
        ) : step.value === "license" ? (
          <section class="wizard-pane">
            <h1>{s.license.title}</h1>
            <p class="wizard-sub">{s.license.sub}</p>
            <HkScrollContainer class="license-box" axis="vertical">
              <div
                class="license-doc"
                innerHTML={renderRichText(
                  agreementDocs.value[licenseIndex.value]?.body ?? "",
                )}
              />
            </HkScrollContainer>
            {agreementDocs.value.length > 1 && (
              <div class="license-pager">
                <HkButton
                  variant="ghost"
                  size="sm"
                  disabled={licenseIndex.value <= 0}
                  ariaLabel={s.license.prevDoc}
                  onClick={() => (licenseIndex.value -= 1)}
                >
                  <ChevronLeft size={15} />
                </HkButton>
                <span class="license-pager__label">
                  {licenseIndex.value + 1}/{agreementDocs.value.length}{" "}
                  {agreementDocs.value[licenseIndex.value]?.title ?? ""}
                </span>
                <HkButton
                  variant="ghost"
                  size="sm"
                  disabled={licenseIndex.value >= agreementDocs.value.length - 1}
                  ariaLabel={s.license.nextDoc}
                  onClick={() => (licenseIndex.value += 1)}
                >
                  <ChevronRight size={15} />
                </HkButton>
              </div>
            )}
            <HkCheckbox
              modelValue={agreed.value}
              label={s.license.agree}
              onUpdate:modelValue={(v: boolean) => (agreed.value = v)}
            />
          </section>
        ) : step.value === "install" ? (
          <section class="wizard-pane wizard-pane--install">
            <div class="wizard-install__main">
              <img src={logoUrl.value} alt="" class="wizard-logo" />
              <p class="wizard-pane__title">{product.value}</p>
              <HkProgressBar
                status="loading"
                size="md"
                value={overall.value ?? undefined}
                showLabel={overall.value != null}
              />
              <p class="wizard-step">{flowStep.value || s.install.fallback}</p>
            </div>
            <div class="wizard-install__logs">
              <LogPane
                lines={logLines.value}
                labels={s.logPane}
                order={logOrder.value}
                expanded={logExpanded.value}
                onToggleExpanded={() => {
                  logExpanded.value = !logExpanded.value;
                }}
              />
            </div>
          </section>
        ) : installFailed.value ? (
          <section class="wizard-pane wizard-done">
            <XCircle
              size={56}
              color="rgb(var(--color-error))"
              stroke-width={1.5}
            />
            <p class="wizard-done__title wizard-done__title--fail">{s.done.failedTitle}</p>
            <p class="wizard-done__error">{failMessage.value}</p>
            <div class="wizard-install__logs">
              <LogPane
                lines={logLines.value}
                labels={s.logPane}
                order={logOrder.value}
                expanded={logExpanded.value}
                onToggleExpanded={() => {
                  logExpanded.value = !logExpanded.value;
                }}
              />
            </div>
            <div class="wizard-done__actions">
              <HkButton variant="primary" onClick={start}>
                {s.done.retry}
              </HkButton>
              <HkButton variant="ghost" onClick={() => tauriWindow()?.close()}>
                {s.done.close}
              </HkButton>
            </div>
          </section>
        ) : (
          <section class="wizard-pane wizard-done">
            <CheckCircle2
              size={56}
              color="rgb(var(--color-success))"
              stroke-width={1.5}
            />
            <p class="wizard-done__title">{s.done.title}</p>
            <p class="wizard-done__path">{dir.value.trim()}</p>
            <p class="wizard-done__hint">
            {s.done.hintLocal}
            </p>
            <div class="wizard-done__shortcuts">
              <>
                <HkCheckbox
                  modelValue={startMenuShortcut.value}
                  label={s.done.shortcutMenu}
                  onUpdate:modelValue={(v: boolean) => toggleMenu(v)}
                />
                <HkCheckbox
                  modelValue={desktopShortcut.value}
                  label={s.done.shortcutDesktop}
                  onUpdate:modelValue={(v: boolean) => toggleDesktop(v)}
                />
              </>
              <HkCheckbox
                modelValue={launchAfterInstall.value}
                label={s.done.launchAfter}
                onUpdate:modelValue={(v: boolean) => (launchAfterInstall.value = v)}
              />
            </div>
          </section>
        );

      return (
        <>
          <HkWallpaperBackdrop />
          <AppTitleBar
            icon={logoUrl.value}
            title={s.title}
            subtitle={identity.value ? `v${identity.value.version}` : ""}
            showMaximize={false}
            // The light/dark toggle rides the caption's custom actions,
            // left of minimize — the same seat the egui face gives it.
            customActions={
              theme.value?.["user-adjustable"]
                ? [
                    {
                      id: "theme",
                      label: s.themeToggle,
                      icon:
                        themeMode.value === "dark" ? (
                          <Sun size={14} />
                        ) : (
                          <Moon size={14} />
                        ),
                    },
                  ]
                : []
            }
            onAction={(id: string) => {
              if (id === "theme") toggleTheme();
            }}
          />
          <main class="installer" style={pageStyle.value}>
            <div class="wizard-layout wizard-layout--left">
              <div class="wizard-layout__rail" style={railStyle.value}>
                <HkTimeline
                  steps={timelineSteps}
                  currentKey={step.value}
                  orientation="vertical"
                />
              </div>
              <div class="wizard-layout__pane" style={paneStyle.value}>{pane}</div>
            </div>

            {note.value && (
              <HkAlert
                variant={note.value.kind === "err" ? "error" : "success"}
                message={note.value.text}
                banner
              />
            )}

            <footer class="installer__footer">
              <div class="installer__nav">
                {running.value ? null : (step.value === "language") && (
                  <HkButton variant="primary" size="lg" onClick={() => go("mode")}>
                    {s.nav.next}
                  </HkButton>
                )}
                {running.value ? null : step.value === "mode" && (
                  <>
                    <HkButton variant="ghost" onClick={() => go("language")}>
                      {s.nav.back}
                    </HkButton>
                    <HkButton
                      variant="primary"
                      size="lg"
                      disabled={dirWritable.value === false}
                      onClick={() => go(flow[flowIndex("mode") + 1].key)}
                    >
                      {s.nav.next}
                    </HkButton>
                  </>
                )}
                {step.value.startsWith("content:") && (
                  <>
                    <HkButton
                      variant="ghost"
                      onClick={() => go(flow[flowIndex(step.value) - 1].key)}
                    >
                      {s.nav.back}
                    </HkButton>
                    <HkButton
                      variant="primary"
                      size="lg"
                      onClick={() => go(flow[flowIndex(step.value) + 1].key)}
                    >
                      {s.nav.next}
                    </HkButton>
                  </>
                )}
                {step.value === "pairing" && (
                  <>
                    <HkButton
                      variant="ghost"
                      onClick={() => go(flow[flowIndex("pairing") - 1].key)}
                    >
                      {s.nav.back}
                    </HkButton>
                    <HkButton
                      variant="primary"
                      size="lg"
                      disabled={!pairingClaimed.value}
                      onClick={() => go(flow[flowIndex("pairing") + 1].key)}
                    >
                      {s.nav.next}
                    </HkButton>
                  </>
                )}
                {step.value === "license" && (
                  <>
                    <HkButton
                      variant="ghost"
                      onClick={() => go(flow[flowIndex("license") - 1].key)}
                    >
                      {s.nav.back}
                    </HkButton>
                    <HkButton
                      variant="primary"
                      size="lg"
                      disabled={!agreed.value || noticeCountdown.value > 0}
                      onClick={start}
                    >
                      {s.license.agreeInstall(noticeCountdown.value)}
                    </HkButton>
                  </>
                )}
                {step.value === "done" && !installFailed.value && (
                  <HkButton
                    variant="primary"
                    size="lg"
                    disabled={finishing.value}
                    onClick={finish}
                  >
                    {s.done.finish}
                  </HkButton>
                )}
              </div>
            </footer>
          </main>
        </>
      );
    };
  },
});
