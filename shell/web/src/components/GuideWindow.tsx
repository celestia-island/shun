import { computed, defineComponent, ref, watch, type PropType } from "vue";
import {
  HkErrorLanding,
  HkMarkdownRenderer,
  HkModal,
  type ModalAction,
} from "@celestia-island/hikari";

import { isInstallerLocale, type InstallerLocale } from "../i18n";
import { invoke } from "../tauri";

import "./GuideWindow.scss";

/**
 * The installer's help window — the "what do I do now" surface behind every
 * classified failure. It is a hikari WINDOW (HkModal), not a hand-rolled
 * overlay: the popup manager owns its z band, nested windows stack above it,
 * ESC/backdrop/X close it, and the body scroller is the modal's own (one
 * scrollbar per window, the family rule).
 *
 * ONE TOPIC PER WINDOW. The topic ids mirror `shun::help::TOPICS` (the core
 * crate names them; this face only renders them):
 *
 *   app-control · permissions · disk-space · file-in-use · path-too-long ·
 *   network · missing-files
 *
 * plus the `error` pseudo-topic: the generic detail window for a failure
 * that has no official page (the `io` / `other` kinds), which renders the
 * raw reason through the standard inline error landing instead of an
 * article.
 *
 * The articles are the short, plain-language markdown under
 * `shell/guide/<topic>/<locale>.md` (first line = `# <title>`), authored
 * beside the shell crate so both faces embed the same documents. They are
 * bundled as raw markdown at build time (one eager `import.meta.glob` over
 * the whole tree — a static asset set, no runtime fetch), with the
 * locale → zh-Hans → en fallback chain PER TOPIC. A topic whose files have
 * not been authored yet renders the caller's `fallbackBody` (the toast
 * headline) rather than an empty window.
 *
 * Every article window also carries the official page: on open it asks the
 * backend for `get_help_url(topic, locale)` and, when the platform has one,
 * seats an "open the official page" action in the footer that calls
 * `open_help_url` (the system browser). Where no official page exists (the
 * `missing-files` family), the footer says so instead of hiding the fact.
 */
export type GuideTopic =
  | "app-control"
  | "permissions"
  | "disk-space"
  | "file-in-use"
  | "path-too-long"
  | "network"
  | "missing-files"
  | "error";

/** Every topic id the face accepts — the runtime guard for ids that arrive
 *  from the backend (`helpTopic`) or from a preview query. */
export const GUIDE_TOPICS: readonly GuideTopic[] = [
  "app-control",
  "permissions",
  "disk-space",
  "file-in-use",
  "path-too-long",
  "network",
  "missing-files",
  "error",
];

/** Narrow an untyped id (backend payload / query string) to a topic. */
export function isGuideTopic(value: unknown): value is GuideTopic {
  return typeof value === "string" && (GUIDE_TOPICS as readonly string[]).includes(value);
}

/** `?raw` bodies of the authored articles, keyed by their source path
 *  (`…/guide/<topic>/<locale>.md`). Resolved by Vite at build time; a
 *  missing file simply has no entry (the fallback chain covers it). */
const GUIDE_SOURCES = import.meta.glob<string>("../../../guide/*/*.md", {
  query: "?raw",
  import: "default",
  eager: true,
});

/** topic id → locale → raw markdown. */
const GUIDE_BY_TOPIC: Record<string, Partial<Record<InstallerLocale, string>>> = {};
for (const [path, raw] of Object.entries(GUIDE_SOURCES)) {
  const parts = path.split("/");
  const locale = parts[parts.length - 1].replace(/\.md$/i, "");
  const topic = parts[parts.length - 2];
  if (!topic || !isInstallerLocale(locale)) continue;
  (GUIDE_BY_TOPIC[topic] ??= {})[locale] = raw;
}

/** Split the leading `# <title>` line off an article: the window's title is
 *  the article's own headline, the rest is the body the markdown renderer
 *  receives. An article that skips the heading keeps its whole text as the
 *  body and falls back to the caller's title. */
export function splitGuide(raw: string | undefined): { title: string; body: string } {
  if (!raw) return { title: "", body: "" };
  const lines = raw.replace(/^\uFEFF/, "").split(/\r?\n/);
  let start = 0;
  while (start < lines.length && lines[start].trim() === "") start += 1;
  const heading = /^#\s+(.+)$/.exec(lines[start] ?? "");
  if (heading) {
    return {
      title: heading[1].trim(),
      body: lines.slice(start + 1).join("\n"),
    };
  }
  return { title: "", body: lines.slice(start).join("\n") };
}

/** Resolve an article for a topic + locale (locale → zh-Hans → en). */
function guideFor(topic: string, locale: InstallerLocale): { title: string; body: string } {
  const byLocale = GUIDE_BY_TOPIC[topic];
  return splitGuide(byLocale?.[locale] ?? byLocale?.["zh-Hans"] ?? byLocale?.en);
}

export default defineComponent({
  name: "GuideWindow",
  props: {
    modelValue: { type: Boolean, required: true },
    /** The wizard locale — the article follows it live, like every other
     *  pane. */
    locale: { type: String as PropType<InstallerLocale>, default: "zh-Hans" },
    topic: { type: String as PropType<GuideTopic>, default: "app-control" },
    /** Window title when the article carries no `# ` heading. */
    fallbackTitle: { type: String, default: "" },
    /** Body stand-in while the topic's article is not authored yet: the
     *  window still says what happened instead of rendering empty. */
    fallbackBody: { type: String, default: "" },
    /** Generic (`error`) mode: window title, the landing's headline, the
     *  raw backend message, and the full copy payload for the built-in
     *  copy action. */
    errorTitle: { type: String, default: "" },
    errorHeadline: { type: String, default: "" },
    errorMessage: { type: String, default: "" },
    errorCopyText: { type: String, default: "" },
    /** Footer labels (the caller's i18n). */
    closeLabel: { type: String, default: "" },
    /** Secondary footer action: open the topic's official vendor page. */
    openOfficialLabel: { type: String, default: "" },
    /** Footer note where the platform has no official page for the topic. */
    contactDeveloper: { type: String, default: "" },
    /** Preview override for the official-page probe (a browser has no IPC):
     *  `"none"` simulates a topic without a page, any other value is used as
     *  the resolved URL. Unset = ask the backend. */
    helpPreview: { type: String, default: undefined },
  },
  emits: {
    "update:modelValue": (_value: boolean) => true,
  },
  setup(props, { emit }) {
    const guide = computed(() => guideFor(props.topic, props.locale));
    const isArticle = computed(() => props.topic !== "error");
    const title = computed(() =>
      isArticle.value
        ? guide.value.title || props.fallbackTitle
        : props.errorTitle || props.fallbackTitle,
    );
    const articleBody = computed(() => {
      if (!isArticle.value) return "";
      const body = guide.value.body.trim();
      return body !== "" ? guide.value.body : props.fallbackBody;
    });

    // The official page for this topic: resolved on open (and on a locale
    // switch) from the ONE table the core crate owns — the article's footer
    // and the online-guides mode therefore never disagree about the URL.
    // `null` = the platform has no page for this topic (e.g. missing-files),
    // `undefined` = the probe has not answered yet.
    // `undefined` = still unknown: neither the action nor the "no page" note
    // renders, so a refused probe never claims a page does not exist.
    const helpUrl = ref<string | null | undefined>(undefined);
    function probeHelpUrl() {
      if (!isArticle.value) {
        helpUrl.value = null;
        return;
      }
      const preview = props.helpPreview;
      if (preview !== undefined) {
        helpUrl.value = preview === "none" ? null : preview;
        return;
      }
      invoke<string | null>("get_help_url", { topic: props.topic, locale: props.locale })
        .then((url) => {
          helpUrl.value = url ?? null;
        })
        .catch(() => {
          // No Tauri bridge (browser preview) or a refused probe: leave the
          // answer unknown — the article stands on its own and the footer
          // says nothing it cannot back up.
          helpUrl.value = undefined;
        });
    }
    watch(
      () => props.modelValue,
      (open) => {
        if (open) probeHelpUrl();
      },
      { immediate: true },
    );
    watch(
      [() => props.topic, () => props.locale],
      () => {
        if (props.modelValue) probeHelpUrl();
      },
    );

    function openOfficialPage() {
      void invoke<boolean>("open_help_url", { topic: props.topic, locale: props.locale }).catch(
        (err) => {
          console.warn(`[shun] opening the official help page for "${props.topic}" failed:`, err);
        },
      );
    }

    const footerActions = computed<ModalAction[]>(() => {
      const actions: ModalAction[] = [];
      if (isArticle.value && helpUrl.value) {
        actions.push({
          label: props.openOfficialLabel,
          variant: "secondary",
          onClick: openOfficialPage,
        });
      }
      actions.push({
        label: props.closeLabel,
        variant: "ghost",
        onClick: () => emit("update:modelValue", false),
      });
      return actions;
    });

    /** The footer's lead note: an article whose platform has no official
     *  page says so (and names the fallback channel) instead of silently
     *  offering a single button. */
    const showNoPageNote = computed(
      () => isArticle.value && helpUrl.value === null && props.contactDeveloper !== "",
    );

    return () => (
      <HkModal
        modelValue={props.modelValue}
        onUpdate:modelValue={(value: boolean) => emit("update:modelValue", value)}
        title={title.value}
        // 40rem: the egui face's help window is a 640px card and the two
        // faces must read as the same secondary window — `lg` (56rem)
        // renders near-full-screen in the 800px installer surface.
        width="md"
        // Header-less naming fallback: the shared popup layer and the
        // dialog's aria-label stay named even if the article has no heading.
        surfaceTitle={title.value || props.fallbackTitle}
        contentClass="shun-guide-window"
        footerActions={footerActions.value}
      >
        {{
          default: () =>
            isArticle.value ? (
              <HkMarkdownRenderer content={articleBody.value} />
            ) : (
              <HkErrorLanding
                variant="inline"
                tone="error"
                title={props.errorHeadline || props.errorTitle}
                description={props.errorMessage}
                copyText={props.errorCopyText}
              />
            ),
          footerLead: () =>
            showNoPageNote.value ? (
              <span class="shun-guide-window__no-page">{props.contactDeveloper}</span>
            ) : null,
        }}
      </HkModal>
    );
  },
});
