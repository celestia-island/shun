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
  HkButton,
  HkCheckbox,
  HkErrorLanding,
  HkJsonTree,
  HkProgressBar,
  HkScrollContainer,
  HkSelect,
  HkTimeline,
  setLocale as setHikariLocale,
  useToast,
} from "@celestia-island/hikari";
import HkWallpaperBackdrop from "@celestia-island/hikari/components/HkWallpaperBackdrop";
import {
  initWallpaper,
  useWallpaper,
} from "@celestia-island/hikari/theme/useWallpaper";

import { composeAgreementDocs } from "./agreementDoc";
import { solarWantsDark, startSolarClock } from "./solar";
import AppTitleBar from "./components/AppTitleBar";
import PathField, { type DriveInfo } from "./components/PathField";
import LogPane, { type LogLine } from "./components/LogPane";
import GuideWindow, {
  GUIDE_TOPICS,
  isGuideTopic,
  type GuideTopic,
} from "./components/GuideWindow";
import {
  isInstallerLocale,
  LOCALE_OPTIONS,
  resolveSystemLocale,
  strings,
  type InstallerLocale,
  type InstallerStrings,
  type NoticeStrings,
} from "./i18n";
import { renderRichText } from "./richText";
import { invoke, listen, openDirectory, tauriWindow } from "./tauri";

/**
 * The only sanctioned bridge from manifest-controlled rich text to
 * `innerHTML`: `renderRichText` HTML-escapes the WHOLE source before
 * any transform runs (see ./richText.ts), so the output carries only
 * the renderer's own tags and every document body rendered below is
 * inert markup. Route every rich-text sink through this call — a bare
 * `renderRichText(...)` at an `innerHTML` sink hides the boundary.
 */
function sanitizedRichText(body: string): string {
  return renderRichText(body);
}

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
type StepKey = "language" | "mode" | "license" | "install" | "done" | `content:${string}`;

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

/**
 * The classified failure the backend hands this face, in both directions:
 * `finish_install` REJECTS with it, and `classify_error` RETURNS it for a
 * flow error that arrives as text (the failed-install page). It is the Rust
 * `shun::failure::FinishFailure` serialized over Tauri's invoke channel —
 * camelCase field names, kebab-case enum values.
 *
 * `stage` names which part of the flow failed (the install flow itself, the
 * shortcut choices, or the immediate launch) and `kind` is the narrow family
 * the face branches on. Every family except `io` / `other` carries a help
 * topic (`shun::help::TOPICS`), i.e. an article of its own; the two bare
 * families get the generic detail window.
 */
interface FinishFailurePayload {
  stage?: string;
  kind?: string;
  message?: string;
  /** Raw OS error (`4551` / 0x11C7 for an application-control block). */
  osError?: number | null;
  /** Help topic the backend names for this failure. */
  helpTopic?: string | null;
}

/** Which part of the flow failed. */
type FailureStage = "install" | "shortcuts" | "launch";

/** The failure families the backend classifies (`shun::failure::FailureKind`). */
type FailureKind =
  | "app-control-blocked"
  | "missing-executable"
  | "permissions"
  | "disk-full"
  | "file-in-use"
  | "path-too-long"
  | "network"
  | "io"
  | "other";

const FAILURE_KINDS: readonly FailureKind[] = [
  "app-control-blocked",
  "missing-executable",
  "permissions",
  "disk-full",
  "file-in-use",
  "path-too-long",
  "network",
  "io",
  "other",
];

const FAILURE_STAGES: readonly FailureStage[] = ["install", "shortcuts", "launch"];

/** The normalized failure every web surface branches on. */
interface ClassifiedFinish {
  stage: FailureStage;
  kind: FailureKind;
  message: string;
  osError: number | null;
  helpTopic: string | null;
}

/**
 * OS codes that mean "the OS refused to create this process" and therefore
 * share the application-control remedy (`shun::failure` constants):
 * `ERROR_APPLICATION_CONTROL_POLICY_BLOCKED` (0x11C7 / 4551, Smart App
 * Control) and the legacy `ERROR_ACCESS_DISABLED_BY_POLICY` (0x4EC / 1260,
 * AppLocker / WDAC group-policy launchers).
 */
const APP_CONTROL_OS_ERRORS = [4551, 1260];

/** The help topic the backend names for an application-control block. */
const APP_CONTROL_HELP_TOPIC = "app-control";

/**
 * Backend help topic → failure family. Used when a rejection names the topic
 * but not the kind (a legacy string rejection carrying the message of a
 * classified family), so the face still routes to the right article.
 */
const HELP_TOPIC_KIND: Record<string, FailureKind> = {
  [APP_CONTROL_HELP_TOPIC]: "app-control-blocked",
  permissions: "permissions",
  "disk-space": "disk-full",
  "file-in-use": "file-in-use",
  "path-too-long": "path-too-long",
  network: "network",
  "missing-files": "missing-executable",
};

/**
 * Failure family → the help article it opens. The ids mirror
 * `shun::help::TOPICS`; `io` / `other` are absent on purpose — no official
 * page exists for "something went wrong", so those keep the generic detail
 * window (raw message + copy).
 */
const KIND_TOPIC: Partial<Record<FailureKind, GuideTopic>> = {
  "app-control-blocked": "app-control",
  permissions: "permissions",
  "disk-full": "disk-space",
  "file-in-use": "file-in-use",
  "path-too-long": "path-too-long",
  network: "network",
  "missing-executable": "missing-files",
};

/**
 * `&notice=<id>` → the failure family the preview pushes. The ids match the
 * help topics where one exists (so `&topic=` and `&notice=` line up), the
 * two legacy #167 ids still work for the generic faces, and every classified
 * family has an id of its own so each article's toast is capturable.
 */
const PREVIEW_NOTICE_KINDS: Record<string, FailureKind> = {
  "app-control": "app-control-blocked",
  permissions: "permissions",
  "disk-full": "disk-full",
  "file-in-use": "file-in-use",
  "path-too-long": "path-too-long",
  network: "network",
  "missing-files": "missing-executable",
  "launch-failed": "other",
  "shortcuts-failed": "other",
};

/** Recover an OS error code from a legacy string rejection — the old
 *  `launch_app` surfaced `io error: … (os error 4551)`, and some paths
 *  carry the hex form (0x11C7) instead. */
function osErrorFromMessage(message: string): number | null {
  const decimal = /\bos error (\d+)\b/i.exec(message);
  if (decimal) return Number(decimal[1]);
  const hex = /\b0x([0-9a-f]{1,8})\b/i.exec(message);
  if (hex) return Number.parseInt(hex[1], 16);
  return null;
}

/** Normalize an untyped rejection kind: the classified value wins, a legacy
 *  string is recovered from its OS code / help topic. */
function normalizeFinishKind(
  kind: unknown,
  osError: number | null,
  helpTopic: unknown,
): FailureKind {
  if (typeof kind === "string" && (FAILURE_KINDS as readonly string[]).includes(kind)) {
    return kind as FailureKind;
  }
  if (osError !== null && APP_CONTROL_OS_ERRORS.includes(osError)) return "app-control-blocked";
  if (typeof helpTopic === "string" && helpTopic in HELP_TOPIC_KIND) {
    return HELP_TOPIC_KIND[helpTopic];
  }
  return "other";
}

/** Normalize an untyped stage: a bare string rejection defaults to the
 *  install lane (the flow's own errors are the strings that arrive here). */
function normalizeFinishStage(stage: unknown, message: string): FailureStage {
  if (typeof stage === "string" && (FAILURE_STAGES as readonly string[]).includes(stage)) {
    return stage as FailureStage;
  }
  if (/shortcut/i.test(message)) return "shortcuts";
  return "install";
}

/**
 * Defensive normalization of a classified rejection. The commands hand back
 * the classified object; a bare string (a legacy backend whose commands
 * returned plain messages) and a plain `Error` (no Tauri bridge — a browser
 * preview, a dead IPC) are still classified instead of surfacing
 * `[object Object]`.
 *
 * `fallbackStage` is the lane whose failure this is when the payload carries
 * no stage: `finish_install` rejects from the done page (launch), while a
 * bare `start_install` rejection is the install flow's.
 */
function classifyFinishError(
  raw: unknown,
  fallbackStage: FailureStage = "launch",
): ClassifiedFinish {
  const payload =
    raw !== null && typeof raw === "object" && !(raw instanceof Error)
      ? (raw as FinishFailurePayload)
      : null;
  const message =
    payload && typeof payload.message === "string"
      ? payload.message
      : raw instanceof Error
        ? raw.message
        : String(raw);
  const osError =
    payload && typeof payload.osError === "number"
      ? payload.osError
      : osErrorFromMessage(message);
  const kind = normalizeFinishKind(payload?.kind, osError, payload?.helpTopic);
  const stage =
    payload?.stage !== undefined
      ? normalizeFinishStage(payload.stage, message)
      : /shortcut/i.test(message)
        ? "shortcuts"
        : fallbackStage;
  // The backend names the topic; a legacy payload that carries none still
  // gets one from the family table (the table is the same mapping the core
  // crate applies, so the two cannot disagree about which article to open).
  const helpTopic =
    typeof payload?.helpTopic === "string" && payload.helpTopic !== ""
      ? payload.helpTopic
      : (KIND_TOPIC[kind] ?? null);
  return { stage, kind, message, osError, helpTopic };
}

/** The plain-language headline for a TOPIC — what the help window shows as
 *  its title and stand-in body while the topic's article is not authored
 *  yet, so the window still says what happened instead of rendering empty. */
function topicHeadline(topic: GuideTopic, notices: NoticeStrings): string {
  switch (topic) {
    case "app-control":
      return notices.launchBlocked;
    case "permissions":
      return notices.permissions;
    case "disk-space":
      return notices.diskFull;
    case "file-in-use":
      return notices.fileInUse;
    case "path-too-long":
      return notices.pathTooLong;
    case "network":
      return notices.network;
    case "missing-files":
      return notices.missingFiles;
    default:
      return notices.errorTitle;
  }
}

/** The plain-language headline for a classified failure — one per family,
 *  short on purpose: it becomes the toast text, and the toast clamps to
 *  three lines. */
function failureHeadline(failure: ClassifiedFinish, notices: NoticeStrings): string {
  switch (failure.kind) {
    case "app-control-blocked":
      return notices.launchBlocked;
    case "missing-executable":
      return notices.missingFiles;
    case "permissions":
      return notices.permissions;
    case "disk-full":
      return notices.diskFull;
    case "file-in-use":
      return notices.fileInUse;
    case "path-too-long":
      return notices.pathTooLong;
    case "network":
      return notices.network;
    default:
      if (failure.stage === "shortcuts") return notices.shortcutsFailed;
      if (failure.stage === "install") return notices.errorTitle;
      return notices.launchFailed;
  }
}

// Quick-candidate row: label + glyph per candidate kind; drive candidates
// show the path itself (a row of drive roots reads better than a bare
// "磁盘"). The two product nouns are locale-independent.

// Step keys in rail order (language leads, the wizard's first step); the
// labels resolve from the string table per render so a locale switch
// relabels the timeline live.
const BASE_STEP_KEYS = ["language", "mode", "license", "install", "done"] as const;

/** One flow entry: the pane key + its timeline label. */
interface FlowStep {
  key: StepKey;
  label: string;
  kind: string;
  /** The resolved pipeline step backing a `content:` pane. */
  step?: { kind: string; title: string; body?: string | null };
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
  }[] | null,
  labels: {
    language: string;
    mode: string;
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
    // query and starts at the language pane. A `?relocate=1` wizard (the
    // Rust side's relocation handover) starts on the location pane — the
    // language question is pre-resolved and the pass owns the pane.
    const query = new URLSearchParams(window.location.search);
    const initialStep = (query.get("step") ?? "") as StepKey;
    const step = ref<StepKey>(
      query.get("relocate") === "1"
        ? "mode"
        : (BASE_STEP_KEYS as readonly string[]).includes(initialStep as never)
          ? (initialStep as StepKey)
          : "language",
    );
    /**
     * Preview hooks (browser / static previews only — production passes no
     * query): a screenshot pass must reach every transient state without a
     * real install. Alongside `?step=`:
     * - `&notice=<id>` pushes that failure's toast, exactly as the shipped
     *   builder would: `app-control` · `permissions` · `disk-full` ·
     *   `file-in-use` · `path-too-long` · `network` · `missing-files` ·
     *   `launch-failed` · `shortcuts-failed`.
     * - `&fail=install` renders the failed-install page (`&topic=` also
     *   names the details seat it opens) — the one state a browser preview
     *   cannot reach on its own, since `classify_error` needs the backend.
     * - `&guides=online` forces the online-guides mode (overrides
     *   `get_config`), so both help modes are capturable: online routes the
     *   toast action straight to `open_help_url` instead of the article.
     * - `&guide=1` opens the help window on mount; `&topic=<id>` picks the
     *   topic (otherwise the pushed notice's topic, else app-control). The
     *   window is opened directly — the mode gate only governs the toast
     *   action, so an online-mode capture can still show the article.
     * - `&help=open|none` fakes the official-page probe inside the article
     *   window (`open` seats the footer action, `none` shows the
     *   "contact the developer" note) — a browser has no IPC to answer it.
     * - `&language=zh-Hans` pins the wizard language (a browser has no saved
     *   preference to restore).
     * - `&product=WoWSP&version=0.6.34` seeds the product/version refs that
     *   normally arrive over Tauri IPC, so the done page and every
     *   `%PRODUCT%` token render real copy in a plain browser. Real IPC
     *   answers overwrite the seeds.
     *
     * Full example: `index.html?step=done&notice=permissions&guide=1&`
     * `topic=permissions&language=zh-Hans&product=WoWSP&version=0.6.34`.
     */
    const previewNotice = query.get("notice");
    const previewGuide = query.get("guide") === "1";
    const previewTopic = query.get("topic");
    const previewGuides = query.get("guides");
    const previewFail = query.get("fail");
    const previewHelp = query.get("help") ?? undefined;
    const previewLanguage = query.get("language");
    const previewProduct = query.get("product");
    const previewVersion = query.get("version");
    const mode = ref<Mode>("local");
    // Wizard locale — resolved synchronously from the system so first paint
    // (and the ?step= previews) always has strings, then overridden by the
    // saved preference once the backend answers (saved > system). A failed
    // invoke (e.g. non-Tauri preview) keeps the system resolution.
    const locale = ref<InstallerLocale>(
      resolveSystemLocale(navigator.language),
    );
    // Preview seed for the wizard language (`&language=zh-Hans`): a browser
    // has no saved preference for the backend to answer with, and a
    // screenshot pass needs to pick the face on demand. Seeded BEFORE the
    // hikari locale watch so the chrome starts on the same language.
    if (previewLanguage && isInstallerLocale(previewLanguage)) {
      locale.value = previewLanguage;
    }
    // The hikari chrome follows the wizard's language: the title bar's
    // caption tooltips (and any other preset-keyed hikari copy) resolve
    // from the preset i18n context, so every locale this wizard shows —
    // the system resolution, the saved-preference restore, the picker —
    // pushes into hikari's setLocale. The wizard's locale ids are a
    // strict subset of hikari's, so the value maps 1:1. Immediate: the
    // system-resolved initial state must land before first paint too,
    // not only the later switches.
    watch(
      locale,
      (value) => {
        void setHikariLocale(value);
      },
      { immediate: true },
    );
    const dir = ref("");
    // The relocation handover (get_relocate_from): the previous install
    // directory the flow sweeps only the product's own files out of.
    // Set when the wizard boots in its relocation pass — the pane shows
    // the notice and the install request carries it.
    const relocateFrom = ref<string | null>(null);
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
    }[]>([]);
    const flashDeclared = ref(false);
    const attachments = ref<
      { key: string; title: string; included: boolean; size: number | null }[]
    >([]);
    const attachmentPicked = ref<Record<string, boolean>>({});
    // Live writability of the shown path: null while the probe is in
    // flight or the box is empty, true/false once the backend answered.
    const dirWritable = ref<boolean | null>(null);
    // The backend's install-dir guard verdict (drive-root refusal plus
    // the manifest's check-dir script hooks): null clears the location;
    // "drive-root"/"non-empty" localize from the table while "script"
    // and "rule" carry the backend's own message verbatim (the script
    // localizes via the wizard language, the rule through its
    // message-locales).
    const dirFault = ref<
      | { kind: "empty" | "not-writable" | "drive-root" | "non-empty" }
      | { kind: "script"; message: string }
      | { kind: "rule"; message: string }
      | null
    >(null);
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
    // Transient notices ride hikari's shared top-right toast stack (one
    // slot per severity, host viewport mounted in main.ts). The in-flow
    // `HkAlert` banner this wizard used to render at the bottom of
    // `main.installer` is RETIRED: it sat under the absolutely-positioned
    // footer and painted over the primary button.
    const toast = useToast();
    // The help window (HkModal) behind every classified failure: one topic
    // per failure family, plus `error` for the families with no official
    // page (io / other — raw message + copy). Both stay hikari windows so the
    // popup manager owns the band and nested windows stack correctly.
    const guideOpen = ref(false);
    const guideTopic = ref<GuideTopic>("app-control");
    const guideError = ref<{
      headline: string;
      message: string;
      copy: string;
    } | null>(null);
    // The classified failure behind the CURRENT help affordance — the
    // online mode's fallback shows its raw message when the vendor hosts
    // no page for the topic.
    const lastFailure = ref<ClassifiedFinish | null>(null);
    // How the faces resolve a help topic (`[shell] guides` in the manifest,
    // reported by get_config): `embedded` opens the in-app article (with the
    // official page one click away), `online` skips the article and opens
    // the vendor page directly. A `&guides=` preview query wins over both.
    const guidesMode = ref<"embedded" | "online">(
      previewGuides === "online" ? "online" : "embedded",
    );
    // The install lane reports its failure as TEXT (`run_install` through
    // `to_string()`), so the failed-install page asks `classify_error` for
    // the family and keeps the topic it names — that is what the page's
    // "details" seat opens.
    const installHelpTopic = ref<GuideTopic | null>(null);

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
    // Row identity for the pane's keys: monotonic, never reused (never reset without clearing logLines in the same statement), so the
    // newest-first list diffs as one insertion per pushed line no matter
    // where the 500-line cap trims (an index key would re-text the whole
    // body on every echo).
    let logSeq = 0;
    const pushLog = (kind: LogLine["kind"], text: string) => {
      logLines.value.push({ key: (logSeq += 1), time: stamp(), kind, text });
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
    // Preview seeds (see the `?product=` / `?version=` hook above): a
    // browser preview has no IPC to ask, and the shipped face always
    // overwrites these from get_config / get_identity.
    if (previewProduct) product.value = previewProduct;
    if (previewVersion) identity.value = { version: previewVersion, flavor: "full" };
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
    watch(dir, (value) => {
      const target = value.trim();
      if (writableTimer !== null) clearTimeout(writableTimer);
      if (!target) {
        dirWritable.value = null;
        dirFault.value = null;
        return;
      }
      dirWritable.value = null;
      dirFault.value = null;
      writableTimer = setTimeout(() => {
        invoke<boolean>("check_dir_writable", { dir: target })
          .then((ok) => {
            if (dir.value.trim() === target) dirWritable.value = ok;
          })
          .catch(() => {
            if (dir.value.trim() === target) dirWritable.value = null;
          });
        // The full guard set (root refusal + non-empty guard + dir
        // rules + check-dir hooks) runs beside the probe; a script hook
        // may take a few milliseconds, and a verdict only lands while it
        // is still about this path.
        void invoke<
          | { kind: "empty" | "not-writable" | "drive-root" | "non-empty" }
          | { kind: "script"; message: string }
          | { kind: "rule"; message: string }
          | null
        >("check_dir", {
          dir: target,
          mode: mode.value,
          language: locale.value,
        })
          .then((fault) => {
            if (dir.value.trim() === target) dirFault.value = fault;
          })
          .catch(() => {
            if (dir.value.trim() === target) dirFault.value = null;
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
      // The relocation handover: when set, this wizard boots in its
      // relocation pass (the step landed on "mode" via ?relocate=1; the
      // backend's default_dir already excludes the old folder from the
      // candidates and preselects a differing one).
      invoke<string | null>("get_relocate_from")
        .then((dir) => {
          relocateFrom.value = dir;
        })
        .catch(() => {});
      refreshDefaults().catch((err) => { hintKind.value = "error"; hintError.value = String(err); });
      invoke<{ version: string; flavor: string }>("get_identity")
        .then((id) => { identity.value = id; })
        .catch(() => {});
      invoke<{ kind: string; data: string } | null>("get_logo")
        .then((logo) => {
          if (logo) logoUrl.value = `data:image/${logo.kind};base64,${logo.data}`;
        })
        .catch(() => {});
      invoke<
        { product: { name: string }; flash?: boolean } & {
          attachments?: { key: string; title: string; included: boolean; size: number | null }[];
        } & { guides_mode?: string; guidesMode?: string }
      >("get_config")
        .then((view) => {
          product.value = view.product?.name ?? "";
          // `guides_mode` is the wire name (the shell view serializes with
          // its Rust field names); the camelCase read is a guard against a
          // future rename, and a `&guides=` preview query outranks both.
          const reported = view.guides_mode ?? view.guidesMode;
          if (previewGuides !== "online" && previewGuides !== "embedded") {
            guidesMode.value = reported === "online" ? "online" : "embedded";
          }
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
          // The page's details seat needs the family (and its help topic);
          // the flow only reports text, so the backend classifies it.
          classifyInstallFailure(event.message);
        }
      });
      // Preview hook landed directly on the license step: arm the notice
      // countdown here too, not only on the wizard's go("license").
      if (step.value === "license") {
        licenseIndex.value = 0;
        startNoticeCountdown();
      }
      // The `?notice=` / `&guide=1` / `&topic=` preview hooks: push exactly
      // the state a real failure would produce (same toast builder, same
      // window), so the screenshot pass captures the shipped surfaces and
      // not a parallel mock-up.
      const previewKind = previewNotice ? PREVIEW_NOTICE_KINDS[previewNotice] : undefined;
      if (previewKind !== undefined) {
        const stage: FailureStage =
          previewNotice === "shortcuts-failed"
            ? "shortcuts"
            : previewNotice === "launch-failed" || previewNotice === "app-control"
              ? "launch"
              : "install";
        showFinishFailure({
          stage,
          kind: previewKind,
          // A real failure carries the OS message; the preview's empty
          // message keeps the toast on the headline, like the topic path.
          message: "",
          osError: previewKind === "app-control-blocked" ? 4551 : null,
          helpTopic: KIND_TOPIC[previewKind] ?? null,
        });
      }
      if (previewFail === "install") {
        // The failed-install page: same state `installFailed` reaches after a
        // real rejection, plus the details seat `classify_error` would name.
        installFailed.value = true;
        failMessage.value = "preview: simulated install failure";
        if (isGuideTopic(previewTopic)) {
          installHelpTopic.value = previewTopic;
          // The production lane arms lastFailure through classify_error;
          // the preview (a dead IPC) arms the same payload so the online
          // fallback renders this failure, not a blank window.
          lastFailure.value = {
            stage: "install",
            kind: "other",
            message: failMessage.value,
            osError: null,
            helpTopic: previewTopic,
          };
        }
      }
      if (previewGuide) {
        // Direct window open (mode-independent) so both modes are
        // capturable; `&topic=` picks the article, else the pushed notice's
        // topic, else app-control.
        openGuideWindow(
          isGuideTopic(previewTopic)
            ? previewTopic
            : previewKind !== undefined
              ? (KIND_TOPIC[previewKind] ?? "app-control")
              : "app-control",
        );
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
        installHelpTopic.value = null;
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
      if (nested !== raw.trim()) {
        // The only explanation of a silently rewritten folder — hold it
        // longer than the 3 s info default so the note is actually read
        // before the install starts. (Caveat, hikari's contract: the
        // duration is read at SLOT-CREATION time, so this holds only
        // while this call creates the face's first `info` slot — true
        // today, as the other pushes are errors.)
        toast.show(withProduct(strings(locale.value)).target.nestedNote, {
          type: "info",
          duration: 8000,
        });
      }
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
          relocateFrom: relocateFrom.value,
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
        classifyInstallFailure(failMessage.value);
        // Failure lands on the done step too: the log lines stay (the
        // drawer auto-expanded on the error record) so the failure trail
        // remains readable next to the retry action. A retry resets them
        // in go("install").
        step.value = "done";
      } finally {
        running.value = false;
      }
    }

    // The done-page confirmation: applies the shortcut choices and the
    // 立即启动 answer in ONE backend call (`finish_install`), then closes
    // the window. The same answer also carries the manifest's
    // post-install companion (`install.post-install-launch`) when the
    // product declares one — a gateway pairer or bootstrap that rides the
    // launch decision instead of its own checkbox. A failure stays in the
    // wizard — the top-right toast names what went wrong and its inline
    // action opens the details window (the OS-remedy guide for an
    // application-control block, the dry facts for everything else) — the
    // user can then fix it and press 完成安装 again.
    async function finish() {
      if (finishing.value) return;
      finishing.value = true;
      try {
        // The backend reports which half failed (`stage`), the narrow
        // reason (`kind` — an application-control block is not an
        // installer failure) and the raw OS code; a rejection is the
        // classified object, normalized defensively below.
        await invoke("finish_install", {
          desktop: desktopShortcut.value,
          menu: startMenuShortcut.value,
          dir: dir.value.trim(),
          launch: launchAfterInstall.value,
        });
        tauriWindow()?.close();
      } catch (err) {
        // The done-page confirmation rejects from the shortcuts/launch
        // lanes; a bare string keeps the launch label as the floor.
        showFinishFailure(classifyFinishError(err, "launch"));
      } finally {
        finishing.value = false;
      }
    }

    /** Routes a classified failure to the top-right toast stack. Every
     *  failure family gets ONE plain-language headline (spoken register,
     *  short — the toast clamps to three lines and the reader is mid-install,
     *  not reading documentation); a family with a help topic is answered by
     *  its own article window, while the two bare families (io / other) keep
     *  the raw message beside the generic detail window. The headline stands
     *  alone whenever a topic carries the detail: a friendly sentence plus
     *  `io error: … (os error 5)` is exactly the noise the copy action and
     *  the article already handle. */
    function showFinishFailure(failure: ClassifiedFinish) {
      lastFailure.value = failure;
      const notices = withProduct(strings(locale.value)).notices;
      const topic = KIND_TOPIC[failure.kind] ?? null;
      const headline = failureHeadline(failure, notices);
      const text =
        topic !== null || failure.message.trim() === ""
          ? headline
          : `${headline} ${failure.message}`;
      // The 30s hold is restated here so the intent sits in the call site
      // (an error card is meant to be read, not dismissed). The stack holds
      // one card per severity and dedupes consecutive identical text, so a
      // retried finish replaces this entry instead of piling duplicates.
      toast.show(text, {
        type: "error",
        copyable: true,
        duration: 30000,
        action: {
          label: notices.details,
          onClick: () => {
            if (topic !== null) {
              openHelp(topic);
            } else {
              openFinishErrorWindow(failure, headline);
            }
          },
        },
      });
    }

    /** Opens a topic's help the way the manifest's `[shell] guides` mode asks
     *  for: the in-app article (embedded), or the official page in the system
     *  browser (online). Everything that surfaces a help affordance routes
     *  through here — the toast action and the failed-install page's details
     *  seat — so the two modes cannot diverge. */
    function openHelp(topic: GuideTopic) {
      if (guidesMode.value === "online") {
        openOnlineHelp(topic);
        return;
      }
      openGuideWindow(topic);
    }

    /** The online-guides mode's whole help experience: the backend resolves
     *  the localized vendor URL and opens the system browser. A vendor with
     *  no page for the topic (missing-files, everywhere) answers `false`,
     *  and a refused browser (no handler, a headless preview) rejects —
     *  both fall back to the details window with the raw reason, exactly
     *  like the egui face, so online mode never swallows the failure. */
    function openOnlineHelp(topic: GuideTopic) {
      const headline = topicHeadline(topic, withProduct(strings(locale.value)).notices);
      const fallback = () =>
        openFinishErrorWindow(
          // The vendor hosts no page for this topic (missing-files,
          // everywhere) or the browser was refused — show the raw reason
          // of the failure that raised the affordance, never nothing.
          lastFailure.value ?? {
            stage: "launch",
            kind: "other",
            message: "",
            osError: null,
            helpTopic: null,
          },
          headline,
        );
      void invoke<boolean>("open_help_url", { topic, locale: locale.value })
        .then((opened) => {
          if (!opened) fallback();
        })
        .catch((err) => {
          console.warn(`[shun] opening the official help page for "${topic}" failed:`, err);
          fallback();
        });
    }

    /** Opens the help window on a topic's article (the embedded mode's
     *  action). The preview hook calls this directly, so a capture can still
     *  show an article under `&guides=online`. */
    function openGuideWindow(topic: GuideTopic) {
      guideTopic.value = topic;
      guideError.value = null;
      guideOpen.value = true;
    }

    /** The help window's fallback face: a family with no article (io /
     *  other) renders the raw reason through the standard inline error
     *  landing, with the full classified payload as its copy text. */
    function openFinishErrorWindow(failure: ClassifiedFinish, headline: string) {
      guideTopic.value = "error";
      guideError.value = {
        headline,
        message: failure.message,
        copy: [
          headline,
          failure.message,
          `stage=${failure.stage} kind=${failure.kind} osError=${failure.osError ?? "-"}`,
          `product=${product.value || "-"} version=${identity.value?.version ?? "-"}`,
        ].join("\n"),
      };
      guideOpen.value = true;
    }

    /** Asks the backend to classify an install-lane failure that arrives as
     *  TEXT (`run_install` reports through `to_string()`), and remembers the
     *  help topic it names — that is what the failed-install page's details
     *  seat opens. Best effort: a dead IPC or an unclassifiable message
     *  simply leaves the page with retry/close. */
    function classifyInstallFailure(message: string) {
      const text = message.trim();
      installHelpTopic.value = null;
      if (!text) return;
      invoke<FinishFailurePayload>("classify_error", { message: text })
        .then((payload) => {
          // ONE normalization path (the same one finish failures take), so
          // the seat's topic and the online fallback's payload cannot
          // drift: the fallback shows THIS failure's raw message.
          const failure = classifyFinishError(payload);
          installHelpTopic.value = isGuideTopic(failure.helpTopic) ? failure.helpTopic : null;
          lastFailure.value = failure;
        })
        .catch(() => {});
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

    // Light/dark resolution (hikari rules, user direction): `system`
    // resolves from the SUN, not the OS preference — the same altitude
    // bands and first-paint estimate the egui face runs (day above
    // +6° → light, civil twilight/night → dark). Longitude is the
    // timezone offset's estimate; the clock re-checks every five
    // minutes so dawn and dusk flip a live wizard. `light`/`dark` pin
    // via [data-mode]; an explicit user toggle (when the manifest
    // allows it) wins for the session and stands the clock down
    // (hikari's `setMode`).
    const themeMode = ref<"light" | "dark">(solarWantsDark() ? "dark" : "light");
    const userPinned = ref<"light" | "dark" | null>(null);
    const applyThemeMode = () => {
      const resolved =
        userPinned.value ?? theme.value?.mode ?? "system";
      // themeMode carries the EFFECTIVE side (system resolves against
      // the sun), so the caption toggle's sun/moon reflects what the
      // page actually shows instead of pinning on the raw "system".
      const effective =
        resolved === "light" || resolved === "dark"
          ? resolved
          : solarWantsDark()
            ? "dark"
            : "light";
      themeMode.value = effective;
      // The verdict always pins [data-mode] explicitly — the
      // prefers-color-scheme blocks in theme.scss are only the pre-JS
      // boot floor now, never the live driver.
      document.documentElement.dataset.mode = effective;
    };
    const toggleTheme = () => {
      userPinned.value = themeMode.value === "dark" ? "light" : "dark";
      applyThemeMode();
    };
    // The solar clock: a `system`-mode face flips at dawn/dusk while
    // the wizard sits open; a session the user pinned ignores it.
    const stopSolarClock = startSolarClock(() => {
      if (userPinned.value === null) applyThemeMode();
    });
    onBeforeUnmount(stopSolarClock);

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
              {/* The standard hikari error landing, the uninstall lane's
                  failed page — same reason/detail/copy treatment. */}
              <div class="wizard-done__landing">
                <HkErrorLanding
                  variant="inline"
                  tone="error"
                  title={
                    uninstallPhase.value === "repair_failed"
                      ? s.uninstall.failedRepair
                      : s.uninstall.failedUninstall
                  }
                  description={uninstallError.value}
                  copyText={`${uninstallPhase.value}\n${uninstallError.value}`}
                >
                  {{
                    default: () => (
                      <HkJsonTree
                        value={{
                          lane: uninstallPhase.value,
                          error: uninstallError.value,
                        }}
                        ariaLabel={s.uninstall.failedUninstall}
                      />
                    ),
                    actions: () => (
                      <HkButton variant="primary" onClick={closeWindow}>
                        {s.uninstall.close}
                      </HkButton>
                    ),
                  }}
                </HkErrorLanding>
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

      const flow = buildFlow(stepsCfg.value, withProduct(strings(locale.value)).steps);
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
              {relocateFrom.value && (
                <div class="wizard-target__relocate">
                  <p class="wizard-target__relocate-title">{s.target.relocateTitle}</p>
                  <p class="wizard-target__relocate-body">{s.target.relocateBody}</p>
                </div>
              )}
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
              {dirFault.value?.kind === "drive-root" && (
                <p class="wizard-target__warning">{s.target.errorRoot}</p>
              )}
              {dirFault.value?.kind === "non-empty" && (
                <p class="wizard-target__warning">{s.target.errorNonEmpty}</p>
              )}
              {dirFault.value?.kind === "script" && (
                <p class="wizard-target__warning">{dirFault.value.message}</p>
              )}
              {dirFault.value?.kind === "rule" && (
                <p class="wizard-target__warning">{dirFault.value.message}</p>
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
              <section class="wizard-pane wizard-pane--doc">
                <h1>{contentStep?.title}</h1>
                <HkScrollContainer class="license-box" axis="vertical">
                  <div
                    class="license-doc"
                    innerHTML={sanitizedRichText(contentStep?.body ?? "")}
                  />
                </HkScrollContainer>
              </section>
            );
          })()
        ) : step.value === "license" ? (
          <section class="wizard-pane wizard-pane--doc">
            <h1>{s.license.title}</h1>
            <p class="wizard-sub">{s.license.sub}</p>
            <HkScrollContainer class="license-box" axis="vertical">
              <div
                class="license-doc"
                innerHTML={sanitizedRichText(
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
            <div class="license-agree">
              <HkCheckbox
                modelValue={agreed.value}
                label={s.license.agree}
                onUpdate:modelValue={(v: boolean) => (agreed.value = v)}
              />
            </div>
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
            {/* The standard hikari error landing — reason verbatim, the
                structured detail as a JSON tree, the built-in copy
                action, the retry/close seats beside it. */}
            <div class="wizard-done__landing">
              <HkErrorLanding
                variant="inline"
                tone="error"
                title={s.done.failedTitle}
                description={failMessage.value}
                copyText={`${s.done.failedTitle}\n${failMessage.value}\n${JSON.stringify(
                  {
                    product: product.value,
                    version: identity.value?.version ?? "",
                    lane: "wizard-install",
                    error: failMessage.value,
                  },
                  null,
                  2,
                )}`}
              >
                {{
                  default: () => (
                    <HkJsonTree
                      value={{
                        product: product.value,
                        version: identity.value?.version ?? "",
                        lane: "wizard-install",
                        error: failMessage.value,
                      }}
                      ariaLabel={s.done.failedTitle}
                    />
                  ),
                  actions: () => (
                    <>
                      <HkButton variant="primary" onClick={start}>
                        {s.done.retry}
                      </HkButton>
                      <HkButton variant="ghost" onClick={() => tauriWindow()?.close()}>
                        {s.done.close}
                      </HkButton>
                      {/* The retry/close pair stays adjacent; the topic seat
                          follows them so a wrapped row never splits it. */}
                      {installHelpTopic.value !== null && (
                        <HkButton
                          variant="ghost"
                          size="sm"
                          onClick={() => openHelp(installHelpTopic.value as GuideTopic)}
                        >
                          {s.notices.details}
                        </HkButton>
                      )}
                    </>
                  ),
                }}
              </HkErrorLanding>
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
                      disabled={dirWritable.value === false || dirFault.value !== null}
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
          {/* The details window behind the toast's inline action: a hikari
              window (popup-manager band, nested-window semantics), never a
              hand-rolled overlay. */}
          <GuideWindow
            modelValue={guideOpen.value}
            onUpdate:modelValue={(value: boolean) => (guideOpen.value = value)}
            locale={locale.value}
            topic={guideTopic.value}
            fallbackTitle={topicHeadline(guideTopic.value, s.notices)}
            fallbackBody={topicHeadline(guideTopic.value, s.notices)}
            errorTitle={s.notices.errorTitle}
            errorHeadline={guideError.value?.headline ?? ""}
            errorMessage={guideError.value?.message ?? ""}
            errorCopyText={guideError.value?.copy ?? ""}
            closeLabel={s.notices.close}
            openOfficialLabel={s.notices.openOfficialPage}
            contactDeveloper={s.notices.contactDeveloper}
            helpPreview={previewHelp}
          />
        </>
      );
    };
  },
});
