import { defineComponent, nextTick, ref, watch } from "vue";

import "./HTerminal.scss";

/**
 * Collapsible monospace output pane — the installer's terminal. The
 * component is deliberately self-contained (state + render, no wizard
 * knowledge): it is same-origin with the egui terminal of the fallback
 * shell and is written to graduate into `@celestia-island/hikari`,
 * where the neighboring external-UI repo reuses it for remote-SSH
 * device consoles and monitoring/reboot landing pages (hover a binary,
 * see its log).
 *
 * New lines snap the view to the tail; quiet stretches leave the user's
 * scroll position alone. Scrollback is bounded client-side by the
 * caller.
 */
export interface TerminalLine {
  kind: "echo" | "step" | "ok" | "error";
  text: string;
}

export default defineComponent({
  name: "HTerminal",
  props: {
    lines: { type: Array<TerminalLine>, required: true },
    title: { type: String, default: "Log" },
    expandLabel: { type: String, default: "Expand log ▾" },
    collapseLabel: { type: String, default: "Collapse log ▴" },
    open: { type: Boolean, default: true },
  },
  setup(props, { expose }) {
    const expanded = ref(props.open);
    const scroller = ref<HTMLElement | null>(null);

    // Snap to the tail whenever fresh lines arrive while expanded.
    watch(
      () => props.lines.length,
      async () => {
        if (!expanded.value) return;
        await nextTick();
        const el = scroller.value;
        if (el) el.scrollTop = el.scrollHeight;
      },
    );

    function toggle() {
      expanded.value = !expanded.value;
      if (expanded.value) {
        void nextTick().then(() => {
          const el = scroller.value;
          if (el) el.scrollTop = el.scrollHeight;
        });
      }
    }

    expose({ toggle });

    return () => (
      <section class="hterminal">
        <header class="hterminal__bar" onClick={toggle}>
          <button type="button" class="hterminal__toggle" tabindex={0}>
            {expanded.value ? props.collapseLabel : props.expandLabel}
          </button>
          <span class="hterminal__title">
            {props.title} · {props.lines.length}
          </span>
        </header>
        {expanded.value && (
          <div class="hterminal__body" ref={scroller}>
            {props.lines.length === 0 ? (
              <span class="hterminal__line hterminal__line--echo">…</span>
            ) : (
              props.lines.map((line, index) => (
                <span
                  key={index}
                  class={`hterminal__line hterminal__line--${line.kind}`}
                >
                  {line.kind === "step"
                    ? "» "
                    : line.kind === "ok"
                      ? "· "
                      : line.kind === "error"
                        ? "× "
                        : "  "}
                  {line.text}
                </span>
              ))
            )}
          </div>
        )}
      </section>
    );
  },
});
