import { computed, defineComponent, onBeforeUnmount, onMounted, ref } from "vue";
import { CheckCircle2, XCircle } from "lucide-vue-next";
import { HkButton, HkProgressBar } from "@celestia-island/hikari";

import AppTitleBar from "./components/AppTitleBar";
import { solarWantsDark, startSolarClock } from "./solar";
import {
  DEFAULT_LOCALE,
  isInstallerLocale,
  resolveSystemLocale,
  strings,
  type InstallerLocale,
} from "./i18n";
import { invoke, listen, tauriWindow } from "./tauri";

/**
 * The silent lane's passive progress window (`[shell] silent-progress`,
 * rendered by the installer's webview lane; index.html?window=passive
 * mounts this instead of the wizard). Same design surface as the wizard:
 * the hikari caption bar (custom title bar — the window is frameless),
 * the theme token stack following `shell.theme` and the solar clock,
 * the hikari progress bar, and copy from the shared string table
 * resolved from the SYSTEM locale — a `--silent` run never picks a
 * wizard language. No pages, no input except the caption close: closing
 * only stops the mirroring, the install itself always runs to
 * completion (the backend worker owns it). Success lingers briefly so
 * the completion line reads, then folds the window; failure holds the
 * error detail plus a close button.
 */

/** What the backend worker reports (serde `kind`-tagged, see
 *  shell/src/passive.rs): a progress step (percent when known), a plain
 *  headline between steps, or the run's final verdict. */
interface PassiveEvent {
  kind: "step" | "note" | "done";
  step?: string;
  percent?: number | null;
  text?: string;
  ok?: boolean;
  failure?: string | null;
}

interface PassiveConfig {
  product: { name: string; version: string };
  theme?: {
    mode?: "system" | "light" | "dark" | null;
  } | null;
}

const DONE_LINGER_MS = 1500;

export default defineComponent({
  name: "PassiveView",
  setup() {
    // Locale: the saved wizard preference (if the wizard ever ran here),
    // else the system locale — the same chain the egui passive window
    // resolves, minus any in-window choice.
    const locale = ref<InstallerLocale>(
      resolveSystemLocale(navigator.language || DEFAULT_LOCALE),
    );
    const uninstallMode = ref(false);
    const logoUrl = ref<string>("/logo.webp");
    const product = ref("");
    const version = ref("");
    const theme = ref<PassiveConfig["theme"]>(null);
    const phase = ref<"running" | "done" | "failed">("running");
    const step = ref("");
    const percent = ref<number | null>(null);
    const failure = ref("");

    // %PRODUCT% splicing — the same deep substitution the wizard does,
    // scoped to the few strings this window shows.
    const splice = (text: string): string =>
      text.replaceAll("%PRODUCT%", product.value);
    const s = computed(() => strings(locale.value));

    // Theme mode: the manifest pin, else the sun — the solar clock's
    // day/night verdict (solar.ts), the same resolution the wizard
    // faces run. The passive window offers no toggle (it is a mirror,
    // not a wizard), so the session pin of App.tsx does not apply; a
    // dawn/dusk crossing mid-run re-applies through the clock.
    const applyThemeMode = () => {
      const resolved = theme.value?.mode ?? "system";
      const dark =
        resolved === "dark" ||
        (resolved !== "light" && solarWantsDark());
      document.documentElement.dataset.mode = dark ? "dark" : "light";
    };
    // The solar clock: a long extraction crossing dawn or dusk flips
    // the mirror with it (the clock owns the re-apply; nothing pins).
    const stopSolarClock = startSolarClock(applyThemeMode);

    const closeWindow = () => {
      tauriWindow()?.close().catch(() => {});
    };

    let lingerTimer: ReturnType<typeof setTimeout> | null = null;
    let gotLiveEvent = false;
    const onEvent = (event: PassiveEvent) => {
      gotLiveEvent = true;
      if (event.kind === "step") {
        step.value = event.step ?? "";
        percent.value = event.percent ?? null;
      } else if (event.kind === "note") {
        step.value = event.text ?? "";
        percent.value = null;
      } else if (event.kind === "done") {
        step.value = "";
        percent.value = null;
        if (event.ok) {
          phase.value = "done";
          lingerTimer = setTimeout(closeWindow, DONE_LINGER_MS);
        } else {
          phase.value = "failed";
          failure.value = event.failure ?? "";
        }
      }
    };

    onMounted(() => {
      invoke<string | null>("get_saved_language")
        .then((saved) => {
          if (isInstallerLocale(saved)) locale.value = saved;
        })
        .catch(() => {});
      invoke<boolean>("is_uninstall_mode")
        .then((flag) => {
          uninstallMode.value = flag;
        })
        .catch(() => {});
      invoke<{ kind: string; data: string } | null>("get_logo")
        .then((logo) => {
          if (logo) logoUrl.value = `data:image/${logo.kind};base64,${logo.data}`;
        })
        .catch(() => {});
      invoke<PassiveConfig>("get_config")
        .then((view) => {
          product.value = view.product?.name ?? "";
          version.value = view.product?.version ?? "";
          theme.value = view.theme ?? null;
          applyThemeMode();
        })
        .catch(() => {});
      listen<PassiveEvent>("passive-progress", onEvent).catch(() => {});
      // Tauri events are not buffered, and the worker started emitting
      // in setup — before this page existed. Replay the lane's coalesced
      // snapshot, but only while no live event has arrived (the snapshot
      // may already be older than what the listener just delivered).
      invoke<PassiveEvent | null>("passive_state")
        .then((snapshot) => {
          if (!gotLiveEvent && snapshot) onEvent(snapshot);
        })
        .catch(() => {});
    });
    onBeforeUnmount(stopSolarClock);

    // The linger timer dies with the window; nothing else needs
    // explicit teardown (the backend owns the worker).

    return () => {
      const table = s.value;
      const heading = splice(
        uninstallMode.value
          ? table.silent.uninstallHeading
          : table.silent.installHeading,
      );
      const doneLine = uninstallMode.value
        ? table.uninstall.doneUninstall
        : table.done.title.replace(/^✔\s*/, "");
      const failedLine = uninstallMode.value
        ? table.uninstall.failedUninstall
        : table.done.failedTitle;
      const running = phase.value === "running";
      return (
        <div class="passive">
          <AppTitleBar
            icon={logoUrl.value}
            title={splice(
              uninstallMode.value ? table.uninstallTitle : table.title,
            )}
            subtitle={version.value ? `v${version.value}` : ""}
            showMaximize={false}
            showMinimize={false}
          />
          <main class="passive-pane">
            <img src={logoUrl.value} alt="" class="passive-logo" />
            {phase.value === "done" ? (
              <>
                <CheckCircle2
                  class="passive-icon"
                  size={44}
                  color="rgb(var(--color-success))"
                  stroke-width={1.5}
                />
                <p class="passive-heading">{doneLine}</p>
              </>
            ) : phase.value === "failed" ? (
              <>
                <XCircle
                  class="passive-icon"
                  size={44}
                  color="rgb(var(--color-error))"
                  stroke-width={1.5}
                />
                <p class="passive-heading">{failedLine}</p>
                {failure.value ? (
                  <p class="passive-failure">{failure.value}</p>
                ) : null}
                <HkButton class="passive-close" onClick={closeWindow}>
                  {table.uninstall.close}
                </HkButton>
              </>
            ) : (
              <>
                <p class="passive-heading">{heading}</p>
                <div class="passive-bar">
                  <HkProgressBar
                    status="loading"
                    size="md"
                    value={percent.value ?? undefined}
                    showLabel={percent.value != null}
                  />
                </div>
                <p class="passive-step">{step.value}</p>
              </>
            )}
          </main>
        </div>
      );
    };
  },
});
