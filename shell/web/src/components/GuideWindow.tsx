import { computed, defineComponent, type PropType } from "vue";
import {
  HkErrorLanding,
  HkMarkdownRenderer,
  HkModal,
  type ModalAction,
} from "@celestia-island/hikari";

import { isInstallerLocale, type InstallerLocale } from "../i18n";

import "./GuideWindow.scss";

/**
 * The installer's detail window — the "what do I do now" surface behind the
 * top-right toast's inline action. It is a hikari WINDOW (HkModal), not a
 * hand-rolled overlay: the popup manager owns its z band, nested windows
 * stack above it, ESC/backdrop/X close it, and the body scroller is the
 * modal's own (one scrollbar per window, the family rule).
 *
 * Two bodies:
 * - `app-control` — the authored per-locale markdown guide for a launch
 *   Windows application-control policy refused (0x11C7 / os error 4551):
 *   the OS remedy, with screenshots. The markdown lives beside the shell
 *   crate (`shell/guide/app-control/<locale>.md`, first line = `# <title>`)
 *   so both installer faces embed the same document; images are served
 *   from `public/guide/app-control-N.png` and referenced relatively.
 * - `error` — every other classified finish failure: the dry facts
 *   (headline + raw backend message) through the standard inline error
 *   landing, whose built-in copy action carries the full payload.
 *
 * The guides are bundled as raw markdown at build time (eager
 * `import.meta.glob` — a static set of assets, no runtime fetch), with a
 * zh-Hans fallback (the original copy) and then English. A missing file
 * for a locale degrades to that fallback instead of an empty window.
 */
export type GuideTopic = "app-control" | "error";

/** `?raw` bodies of the authored guides, keyed by their source path. The
 *  glob is resolved by Vite at build time; a locale without a file simply
 *  has no entry (the fallback chain covers it). */
const GUIDE_SOURCES = import.meta.glob<string>("../../../guide/app-control/*.md", {
  query: "?raw",
  import: "default",
  eager: true,
});

const GUIDE_BY_LOCALE: Partial<Record<InstallerLocale, string>> = {};
for (const [path, raw] of Object.entries(GUIDE_SOURCES)) {
  const name = path.slice(path.lastIndexOf("/") + 1).replace(/\.md$/i, "");
  if (isInstallerLocale(name)) GUIDE_BY_LOCALE[name] = raw;
}

/** Split the leading `# <title>` line off a guide: the window's title is
 *  the guide's own headline, the rest is the body the markdown renderer
 *  receives. A guide that skips the heading keeps its whole text as the
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

/** Resolve the guide for a locale through the fallback chain. */
function guideFor(locale: InstallerLocale): { title: string; body: string } {
  return splitGuide(
    GUIDE_BY_LOCALE[locale] ?? GUIDE_BY_LOCALE["zh-Hans"] ?? GUIDE_BY_LOCALE.en,
  );
}

export default defineComponent({
  name: "GuideWindow",
  props: {
    modelValue: { type: Boolean, required: true },
    /** The wizard locale — the guide body follows it live, like every
     *  other pane. */
    locale: { type: String as PropType<InstallerLocale>, default: "zh-Hans" },
    topic: { type: String as PropType<GuideTopic>, default: "app-control" },
    /** Window title when the guide markdown carries no `# ` heading. */
    fallbackTitle: { type: String, default: "" },
    /** Body stand-in when no guide markdown is bundled yet (the authored
     *  documents land beside the shell crate): the window still says what
     *  happened instead of rendering empty. */
    fallbackBody: { type: String, default: "" },
    /** Generic error mode: window title, the landing's headline, the raw
     *  backend message, and the full copy payload for the built-in copy
     *  action. */
    errorTitle: { type: String, default: "" },
    errorHeadline: { type: String, default: "" },
    errorMessage: { type: String, default: "" },
    errorCopyText: { type: String, default: "" },
    /** Footer action label (the caller's i18n). */
    closeLabel: { type: String, default: "" },
  },
  emits: {
    "update:modelValue": (_value: boolean) => true,
  },
  setup(props, { emit }) {
    const guide = computed(() => guideFor(props.locale));
    const isGuide = computed(() => props.topic === "app-control");
    const title = computed(() =>
      isGuide.value
        ? guide.value.title || props.fallbackTitle
        : props.errorTitle || props.fallbackTitle,
    );
    const guideBody = computed(() => {
      if (!isGuide.value) return "";
      const body = guide.value.body.trim();
      return body !== "" ? guide.value.body : props.fallbackBody;
    });
    const footerActions = computed<ModalAction[]>(() => [
      {
        label: props.closeLabel,
        variant: "ghost",
        onClick: () => emit("update:modelValue", false),
      },
    ]);

    return () => (
      <HkModal
        modelValue={props.modelValue}
        onUpdate:modelValue={(value: boolean) => emit("update:modelValue", value)}
        title={title.value}
        // 40rem: the egui face's guide is a 640px card and the two faces
        // must read as the same secondary window — `lg` (56rem) renders
        // near-full-screen in the 800px installer surface.
        width="md"
        // Header-less naming fallback: the shared popup layer and the
        // dialog's aria-label stay named even if the guide has no heading.
        surfaceTitle={title.value || props.fallbackTitle}
        contentClass="shun-guide-window"
        footerActions={footerActions.value}
      >
        {isGuide.value ? (
          <HkMarkdownRenderer content={guideBody.value} />
        ) : (
          <HkErrorLanding
            variant="inline"
            tone="error"
            title={props.errorHeadline || props.errorTitle}
            description={props.errorMessage}
            copyText={props.errorCopyText}
          />
        )}
      </HkModal>
    );
  },
});
