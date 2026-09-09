import { defineComponent, onBeforeUnmount, onMounted, ref } from "vue";
import { Box, FolderOpen, HardDrive, Monitor } from "lucide-vue-next";
import {
  HAlert,
  HButton,
  HCheckbox,
  HMarkdownRenderer,
  HProgressBar,
  HSelectionGrid,
  HTimeline,
} from "@celestia-island/hikari";

import AppTitleBar from "./components/AppTitleBar";
import HTerminal, { type TerminalLine } from "./components/HTerminal";
import { invoke, listen, openDirectory } from "./tauri";
import { resolveLocale, strings, type Locale } from "./i18n";

/**
 * Shun demo shell UI — an NSIS-style delivery wizard rendered entirely with
 * hikari components. Everything shown is generated from the shun
 * configuration declared in the shell crate's Cargo.toml
 * (`[package.metadata.shun]`) and served by the `get_config` command:
 * product identity, delivery modes, the license page (markdown through
 * HMarkdownRenderer), the timeline orientation (top rail or left rail),
 * theme mode, accent palette, and UI language (eight locales).
 */

type Mode = "local" | "portable";
type StepKey = "mode" | "license" | "install" | "done";

interface ProductIdentity {
  name: string;
  version: string;
  publisher?: string;
  logo?: string;
}

interface DirDefaults {
  dir: string;
}

interface ShellView {
  product: ProductIdentity;
  modes: Mode[];
  timeline?: "top" | "left";
  theme?: { mode?: "system" | "light" | "dark"; accent?: [number, number, number] };
  language?: string;
  log_level?: LogLevel;
  flash: boolean;
}

interface FlowLogRecord {
  type:
    | "file-write"
    | "file-reuse"
    | "script-begin"
    | "script-line"
    | "command-done"
    | "warning";
  path?: string;
  name?: string;
  line?: string;
  command?: string;
  code?: string;
  detail?: string;
}

interface ProgressEvent {
  phase?: "download" | "extract" | "register";
  step?: string;
  percent?: number | null;
  record?: FlowLogRecord;
}

type LogLevel = "all" | "files" | "scripts" | "off";

const STEPS: { key: StepKey; labelKey: string }[] = [
  { key: "mode", labelKey: "step.mode" },
  { key: "license", labelKey: "step.license" },
  { key: "install", labelKey: "step.install" },
  { key: "done", labelKey: "step.done" },
];

export default defineComponent({
  name: "ShunDemoApp",
  setup() {
    const product = ref<ProductIdentity>({ name: "ShunDemo", version: "" });
    const modes = ref<Mode[]>(["local", "portable"]);
    const timeline = ref<"top" | "left">("top");
    const themeMode = ref<"system" | "light" | "dark">("dark");
    const themeAccent = ref<[number, number, number] | null>(null);
    const locale = ref<Locale>("en");
    const mode = ref<Mode>("local");
    const dir = ref("");
    const hint = ref("");

    const step = ref<StepKey>("mode");
    const agreed = ref(false);
    const licenseText = ref("");
    const licenseLoading = ref(true);

    const running = ref(false);
    const installFailed = ref(false);
    const failMessage = ref("");
    const flowStep = ref("");
    const installed = ref(false);

    // Multi-phase progress: one entry per phase seen, updated by phase.
    const phases = ref<Record<string, { percent: number | null; step: string }>>({});
    // Terminal lines + configured verbosity (`shell.log-level`).
    const termLines = ref<TerminalLine[]>([]);
    const logLevel = ref<LogLevel>("all");
    const overall = ref<number | null>(null);
    let noteTimer: number | undefined;
    const note = ref<{ text: string; kind: "ok" | "err" } | null>(null);

    const t = () => strings(locale.value);

    function applyTheme() {
      const dark =
        themeMode.value === "dark" ||
        (themeMode.value === "system" &&
          window.matchMedia("(prefers-color-scheme: dark)").matches);
      document.documentElement.dataset.mode = dark ? "dark" : "light";
    }

    let mediaQuery: MediaQueryList | null = null;
    function onMediaChange() {
      if (themeMode.value === "system") applyTheme();
    }

    function applyAccent(accent: [number, number, number] | null) {
      const root = document.documentElement.style;
      if (accent) {
        const channels = accent.join(" ");
        root.setProperty("--color-primary", channels);
        root.setProperty("--color-focused-border", channels);
        root.setProperty("--color-selected-bg", channels);
      } else {
        root.removeProperty("--color-primary");
        root.removeProperty("--color-focused-border");
        root.removeProperty("--color-selected-bg");
      }
    }

    async function refreshDefaults() {
      const defaults = await invoke<DirDefaults>("default_dir", {
        mode: mode.value,
      });
      dir.value = defaults.dir;
      hint.value = t()["hint." + mode.value] ?? "";
    }

    onMounted(() => {
      invoke<ShellView>("get_config")
        .then((view) => {
          product.value = view.product;
          modes.value = view.modes;
          timeline.value = view.timeline ?? "top";
          themeMode.value = view.theme?.mode ?? "dark";
          themeAccent.value = view.theme?.accent ?? null;
          locale.value = resolveLocale(view.language);
          applyTheme();
          applyAccent(themeAccent.value);
          if (themeMode.value === "system") {
            mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
            mediaQuery.addEventListener("change", onMediaChange);
          }
          if (!view.modes.includes(mode.value)) {
            mode.value = view.modes[0] ?? "local";
          }
          logLevel.value = view.log_level ?? "all";
          return refreshDefaults();
        })
        .catch((err) => {
          hint.value = String(err);
        });
      listen<ProgressEvent>("install-progress", (payload) => {
        if (payload.record) {
          pushLog(payload.record);
          return;
        }
        if (payload.phase && payload.percent != null) {
          phases.value = {
            ...phases.value,
            [payload.phase]: {
              percent: payload.percent ?? 0,
              step: payload.step ?? "",
            },
          };
          // Phase-weighted overall completion: download covers the first
          // tenth, extraction the following 85%.
          const weighted =
            payload.phase === "download"
              ? payload.percent / 10
              : 10 + (payload.percent * 85) / 100;
          overall.value = Math.max(overall.value ?? 0, weighted);
        }
        if (payload.step) flowStep.value = payload.step;
      });
      fetch("/demo-license.md")
        .then((r) => r.text())
        .then((text) => {
          licenseText.value = text;
          licenseLoading.value = false;
        })
        .catch(() => {
          licenseText.value = t()["license.failed"];
          licenseLoading.value = false;
        });
    });

    onBeforeUnmount(() => {
      mediaQuery?.removeEventListener("change", onMediaChange);
    });

    async function selectMode(id: string | number | boolean | undefined) {
      if (running.value) return;
      mode.value = (id as Mode) ?? "local";
      await refreshDefaults().catch((err) => {
        hint.value = String(err);
      });
    }

    async function browse() {
      if (running.value) return;
      const picked = await openDirectory(t()["dir.picker-title"]);
      if (picked) dir.value = picked;
    }

    function go(key: StepKey) {
      step.value = key;
      if (key === "install" && !installed.value) {
        void install();
      }
    }

    /** One structured flow log record → one terminal line, i18n'd,
        honoring `shell.log-level`. */
    function pushLog(record: FlowLogRecord) {
      const strings$ = t();
      if (logLevel.value === "off") return;
      // Warnings bypass the family filter: only `off` hides them.
      if (record.type === "warning") {
        const text =
          record.code === "desktop-shortcut-blocked"
            ? strings$["warn.desktop-blocked"]
            : record.code === "aumid-stamp-blocked"
              ? strings$["warn.aumid-blocked"]
              : (record.detail ?? "");
        termLines.value = [
          ...termLines.value.slice(-1999),
          { kind: "error" as const, text: `⚠ ${text}` },
        ];
        return;
      }
      const script =
        record.type === "script-begin" ||
        record.type === "script-line" ||
        record.type === "command-done";
      if (logLevel.value === "files" && script) return;
      if (logLevel.value === "scripts" && !script) return;
      const line = (() => {
        switch (record.type) {
          case "file-write":
            return { kind: "ok" as const, text: `${strings$["log.write"]} ${record.path ?? ""}` };
          case "file-reuse":
            return { kind: "ok" as const, text: `${strings$["log.reuse"]} ${record.path ?? ""}` };
          case "script-begin":
            return {
              kind: "step" as const,
              text: `${strings$["log.script-begin"]} ${record.name ?? ""}`,
            };
          case "script-line":
            return { kind: "echo" as const, text: record.line ?? "" };
          case "command-done":
            return { kind: "ok" as const, text: `✓ ${record.command ?? ""}` };
        }
      })();
      if (!line) return;
      termLines.value = [...termLines.value.slice(-1999), line];
    }

    async function install() {
      running.value = true;
      installFailed.value = false;
      flowStep.value = t()["install.preparing"];
      phases.value = {};
      termLines.value = [];
      overall.value = 0;
      try {
        await invoke("start_install", {
          mode: mode.value,
          dir: dir.value.trim(),
        });
        overall.value = 100;
        installed.value = true;
        go("done");
      } catch (err) {
        installFailed.value = true;
        failMessage.value = String(err);
      } finally {
        running.value = false;
      }
    }

    async function remove() {
      if (running.value || !installed.value) return;
      running.value = true;
      try {
        await invoke("uninstall_demo", {
          mode: mode.value,
          dir: dir.value.trim(),
        });
        installed.value = false;
        showNote(`✔ ${t()["note.uninstalled"]}：${dir.value.trim()}`);
        step.value = "mode";
      } catch (err) {
        showNote(String(err), "err");
      } finally {
        running.value = false;
      }
    }

    function showNote(text: string, kind: "ok" | "err" = "ok") {
      note.value = { text, kind };
      window.clearTimeout(noteTimer);
      noteTimer = window.setTimeout(() => {
        note.value = null;
      }, 4000);
    }

    return () => {
      const strings$ = t();
      const modeItems = modes.value.map((id) => ({
        id,
        title: strings$[`mode.${id}.title`],
        description: strings$[`mode.${id}.desc`],
        icon: id === "portable" ? HardDrive : Monitor,
      }));
      const timelineSteps = STEPS.map((s) => ({
        key: s.key,
        label: strings$[s.labelKey],
      }));

      const pane =
        step.value === "mode" ? (
          <section class="wizard-pane">
            <h1>
              {product.value.name} {strings$["hero.title.suffix"]}
            </h1>
            <p class="wizard-sub">
              {strings$["hero.version-prefix"]} {product.value.version}
              {product.value.publisher ? ` · ${product.value.publisher}` : ""}
            </p>
            <HSelectionGrid
              items={modeItems}
              selectedId={mode.value}
              columns={2}
              onSelect={(item: { id?: string | number | boolean }) => {
                if (item.id === "local" || item.id === "portable") {
                  void selectMode(item.id);
                }
              }}
            />
            <section class="installer__target">
              <label class="installer__label" for="dir-input">
                {strings$["dir.label"]}
              </label>
              <div class="installer__row">
                <div class="installer__field">
                  <span class="installer__field-icon" aria-hidden="true">
                    <FolderOpen size={18} />
                  </span>
                  <input
                    id="dir-input"
                    type="text"
                    spellcheck={false}
                    v-model={dir.value}
                    disabled={running.value}
                  />
                </div>
                <HButton variant="ghost" disabled={running.value} onClick={browse}>
                  {strings$["dir.browse"]}
                </HButton>
              </div>
              <p class="installer__hint">{hint.value}</p>
            </section>
          </section>
        ) : step.value === "license" ? (
          <section class="wizard-pane">
            <div class="license-box">
              <HMarkdownRenderer
                content={licenseText.value}
                loading={licenseLoading.value}
              />
            </div>
            <HCheckbox
              modelValue={agreed.value}
              label={strings$["license.agree"]}
              onUpdate:modelValue={(v: boolean) => (agreed.value = v)}
            />
          </section>
        ) : step.value === "install" ? (
          <section class="wizard-pane">
            {installFailed.value ? (
              <HAlert
                variant="error"
                title={strings$["install.preparing"]}
                message={failMessage.value}
              />
            ) : (
              <>
                <p class="installer__step">{strings$["install.running"]}</p>
                <HProgressBar
                  status="loading"
                  size="md"
                  value={Math.round(overall.value ?? 0)}
                  showLabel={true}
                />
                {flowStep.value && (
                  <p class="installer__step installer__step--live">
                    {flowStep.value}
                  </p>
                )}
                {logLevel.value !== "off" && (
                  <div class="installer__terminal">
                    <HTerminal
                      lines={termLines.value}
                      title={strings$["log.title"]}
                      expandLabel={strings$["log.expand"]}
                      collapseLabel={strings$["log.collapse"]}
                    />
                  </div>
                )}
              </>
            )}
          </section>
        ) : (
          <section class="wizard-pane wizard-done">
            <p class="wizard-done__title">✔ {strings$["install.done-title"]}</p>
            <p class="wizard-done__path">{dir.value.trim()}</p>
            <p class="installer__hint">
              {mode.value === "portable"
                ? strings$["hint.portable"]
                : strings$["hint.local"]}
            </p>
          </section>
        );

      return (
        <>
          <AppTitleBar icon="/logo.webp" title="Shun Demo Shell" showMaximize={false} />
          <main class="installer">
            <div class={`wizard-layout wizard-layout--${timeline.value}`}>
              <HTimeline
                steps={timelineSteps}
                currentKey={step.value}
                orientation={timeline.value === "left" ? "vertical" : "horizontal"}
              />
              <div class="wizard-layout__pane">{pane}</div>
            </div>

            {note.value && (
              <HAlert
                variant={note.value.kind === "err" ? "error" : "success"}
                message={note.value.text}
                banner
              />
            )}

            <footer class="installer__footer">
              <div>
                {running.value && flowStep.value && (
                  <span class="wizard-live">{flowStep.value}</span>
                )}
              </div>
              <div class="installer__nav">
                {running.value ? null : step.value === "mode" && (
                  <HButton variant="primary" size="lg" onClick={() => go("license")}>
                    {strings$["wizard.next"]}
                  </HButton>
                )}
                {step.value === "license" && (
                  <>
                    <HButton variant="ghost" onClick={() => go("mode")}>
                      {strings$["install.back"]}
                    </HButton>
                    <HButton
                      variant="primary"
                      size="lg"
                      disabled={!agreed.value}
                      onClick={() => go("install")}
                    >
                      {strings$["install.agree-start"]}
                    </HButton>
                  </>
                )}
                {step.value === "install" && installFailed.value && (
                  <HButton variant="primary" onClick={() => go("license")}>
                    {strings$["install.back"]}
                  </HButton>
                )}
                {step.value === "done" && (
                  <>
                    <HButton variant="ghost" onClick={remove} disabled={running.value}>
                      {strings$["install.uninstall"]}
                    </HButton>
                    <HButton
                      variant="primary"
                      size="lg"
                      onClick={() => currentWindow().close()}
                    >
                      {strings$["install.finish"]}
                    </HButton>
                  </>
                )}
              </div>
            </footer>
          </main>
        </>
      );
    };
  },
});

function currentWindow() {
  return (window as unknown as {
    __TAURI__?: { window?: { getCurrentWindow?: () => { close(): Promise<void> } } };
  }).__TAURI__?.window?.getCurrentWindow?.() ?? { close: () => Promise.resolve() };
}
