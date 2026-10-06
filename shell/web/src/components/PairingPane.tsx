import { defineComponent, onBeforeUnmount, onMounted, ref } from "vue";
import { Copy, RefreshCw } from "lucide-vue-next";
import { HkBadge, HkButton } from "@celestia-island/hikari";

import { invoke } from "../tauri";
import type { PairingStrings } from "../i18n";

/**
 * The prefabricated pairing pane (the "inverted lane"): this machine
 * DISPLAYS a code, an operator types it into a control panel, and the
 * credential travels back here.
 *
 * The countdown badge ticks LOCALLY against a wall-clock deadline — the
 * lane's long-poll parks ~20s per answer, so a badge driven only by the
 * server's `expires_in` freezes and then drops in 20-second jumps (the
 * evernight 0.1.47 lesson, now baked into the template). Server answers
 * only re-anchor the deadline.
 */
export default defineComponent({
  name: "PairingPane",
  props: {
    strings: { type: Object, required: true },
    /** The declared pairing step contract, from the resolved pipeline. */
    config: { type: Object, required: true },
    /** Fired when the success card renders — unlocks the wizard's Next. */
    onClaimed: { type: Function, required: false, default: null },
  },
  setup(props) {
    const ps = () => props.strings as PairingStrings;
    const cfg = () => props.config as {
      source: { kind: string; official?: string; allow_custom?: boolean };
      identity?: { "node-id"?: boolean; name?: boolean; tier?: number };
      env_file?: string;
    };

    const phase = ref<"idle" | "requesting" | "waiting" | "claimed" | "error">("idle");
    const error = ref("");
    const code = ref("");
    const ttl = ref(0);
    const deadline = ref(0);
    const copied = ref(false);
    const refreshed = ref(false);
    const claim = ref<{ nodeId: string; owner: string } | null>(null);
    // Identity answers the lane receives (the gateway lane keys on
    // node_id; the scripts lane forwards everything).
    const nodeId = ref(cryptoId());
    const name = ref("");
    const hostname = ref("");
    const gateway = ref("");
    const customGateway = ref(false);

    function cryptoId(): string {
      const bytes = new Uint8Array(8);
      crypto.getRandomValues(bytes);
      return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
    }

    /** Anchors the countdown at `expiresIn` seconds from now; the ticker
     *  keeps it moving between (parked) server answers. */
    function arm(expiresIn: number) {
      deadline.value = Date.now() + Math.max(0, expiresIn) * 1000;
      ttl.value = Math.max(0, Math.round(expiresIn));
    }

    let gen = 0;
    async function start() {
      const mine = ++gen;
      phase.value = "requesting";
      error.value = "";
      refreshed.value = false;
      try {
        const minted = await invoke<{ code: string; expires_in: number }>(
          "pairing_request",
          {
            args: {
              gateway: gateway.value || undefined,
              node_id: nodeId.value,
              name: name.value.trim() || undefined,
              tier: cfg().identity?.tier ?? undefined,
            },
          },
        );
        if (mine !== gen) return;
        code.value = minted.code;
        arm(minted.expires_in);
        phase.value = "waiting";
      } catch (e) {
        if (mine !== gen) return;
        phase.value = "error";
        error.value = String(e);
        return;
      }
      while (mine === gen) {
        let answer: {
          status: string;
          expires_in?: number;
          node_id?: string;
          device_secret?: string;
          owner?: string;
        };
        try {
          answer = await invoke<{
            status: string;
            expires_in?: number;
            node_id?: string;
            device_secret?: string;
            owner?: string;
          }>("pairing_await", {
            args: {
              gateway: gateway.value || undefined,
              node_id: nodeId.value,
              code: code.value,
              name: name.value.trim() || undefined,
              tier: cfg().identity?.tier ?? undefined,
            },
          });
        } catch {
          if (mine !== gen) return;
          // A transient error must not end the wait — the code is still
          // valid on the other side. Back off and retry.
          await new Promise((r) => setTimeout(r, 3000));
          continue;
        }
        if (mine !== gen) return;
        if (answer.status === "claimed" && answer.device_secret) {
          claim.value = {
            nodeId: answer.node_id ?? nodeId.value,
            owner: answer.owner ?? "",
          };
          deadline.value = 0;
          // Persistence failures must NOT hide behind the success card:
          // the scripts lane's `record` is its sole persistence step, and
          // a lost claim on either lane means an unpaired install.
          try {
            await invoke("pairing_claim", {
              outcome: {
                gateway: gateway.value || cfg().source.official || "",
                node_id: answer.node_id ?? nodeId.value,
                device_secret: answer.device_secret,
                owner: answer.owner ?? "",
              },
            });
            await invoke("pairing_record", {
              args: {
                gateway: gateway.value || undefined,
                node_id: answer.node_id ?? nodeId.value,
                device_secret: answer.device_secret,
                owner: answer.owner ?? "",
                pairing_code: code.value,
              },
            });
          } catch (e) {
            phase.value = "error";
            error.value = String(e);
            return;
          }
          // The success card only renders once BOTH persist steps have
          // landed — a failed record must never hide behind it.
          phase.value = "claimed";
          const emit = (props as { onClaimed?: (v: void) => void }).onClaimed;
          emit?.();
          return;
        }
        if (answer.status === "unknown") {
          refreshed.value = true;
          setTimeout(() => (refreshed.value = false), 8000);
          try {
            const again = await invoke<{ code: string; expires_in: number }>(
              "pairing_request",
              {
                args: {
                  gateway: gateway.value || undefined,
                  node_id: nodeId.value,
                  name: name.value.trim() || undefined,
                  tier: cfg().identity?.tier ?? undefined,
                },
              },
            );
            if (mine !== gen) return;
            code.value = again.code;
            arm(again.expires_in);
          } catch (e) {
            if (mine !== gen) return;
            phase.value = "error";
            error.value = String(e);
            return;
          }
        } else if (typeof answer.expires_in === "number") {
          arm(answer.expires_in);
        }
        await new Promise((r) => setTimeout(r, 1000));
      }
    }

    async function copyCode() {
      if (!code.value) return;
      try {
        await navigator.clipboard.writeText(code.value);
        copied.value = true;
        setTimeout(() => (copied.value = false), 2000);
      } catch {
        /* clipboard unavailable — the operator can retype 8 chars */
      }
    }

    onMounted(() => {
      gateway.value = cfg().source.official ?? "";
      void invoke<string>("get_device_hostname")
        .then((host) => (hostname.value = host))
        .catch(() => {});
      void start();
    });
    onBeforeUnmount(() => {
      gen++;
      deadline.value = 0;
    });

    // The local second hand: recomputes the rendered remainder from the
    // armed deadline so the badge moves between parked server answers.
    const ticker = setInterval(() => {
      if (deadline.value > 0) {
        ttl.value = Math.max(
          0,
          Math.round((deadline.value - Date.now()) / 1000),
        );
      }
    }, 1000);
    onBeforeUnmount(() => clearInterval(ticker));

    return () => {
      const s = ps();
      const identity = cfg().identity;
      const allowCustom =
        cfg().source.kind === "gateway" &&
        ((cfg().source as { "allow-custom"?: boolean })["allow-custom"] ?? true);
      return (
        <section class="wizard-pane wizard-pairing">
          {phase.value === "claimed" && claim.value ? (
            <>
              <h1>{s.successTitle}</h1>
              <p class="wizard-sub">{s.successSub}</p>
              <div class="wizard-pairing__card">
                <dl class="wizard-pairing__facts">
                  <div>
                    <dt>{s.successNode}</dt>
                    <dd>{claim.value.nodeId}</dd>
                  </div>
                  <div>
                    <dt>{s.successOwner}</dt>
                    <dd>{claim.value.owner || "—"}</dd>
                  </div>
                </dl>
              </div>
            </>
          ) : (
            <>
              <h1>{s.title}</h1>
              <p class="wizard-sub">{s.sub}</p>
              {identity?.["node-id"] !== false && (
                <div class="wizard-pairing__field">
                  <label class="wizard-pairing__label">{s.nodeIdLabel}</label>
                  <div class="wizard-pairing__row">
                    <code class="wizard-pairing__id">{nodeId.value}</code>
                    <HkButton
                      variant="ghost"
                      size="sm"
                      ariaLabel={s.regenerate}
                      onClick={() => (nodeId.value = cryptoId())}
                    >
                      <RefreshCw size={14} />
                    </HkButton>
                  </div>
                </div>
              )}
              {identity?.name && (
                <div class="wizard-pairing__field">
                  <label class="wizard-pairing__label">{s.nameLabel}</label>
                  <input
                    class="wizard-pairing__input"
                    type="text"
                    v-model={name.value}
                    placeholder={hostname.value || s.namePlaceholder}
                  />
                </div>
              )}
              {allowCustom && (
                <div class="wizard-pairing__field">
                  <label class="wizard-pairing__label">{s.gatewayLabel}</label>
                  <div class="wizard-pairing__row">
                    <HkButton
                      variant={customGateway.value ? "ghost" : "solid"}
                      size="sm"
                      onClick={() => {
                        customGateway.value = false;
                        gateway.value = cfg().source.official ?? "";
                      }}
                    >
                      {s.gatewayOfficial}
                    </HkButton>
                    <HkButton
                      variant={customGateway.value ? "solid" : "ghost"}
                      size="sm"
                      onClick={() => {
                        customGateway.value = true;
                        gateway.value = "";
                      }}
                    >
                      {s.gatewayCustom}
                    </HkButton>
                  </div>
                  {customGateway.value && (
                    <input
                      class="wizard-pairing__input"
                      type="url"
                      v-model={gateway.value}
                      placeholder={s.gatewayPlaceholder}
                    />
                  )}
                </div>
              )}
              {phase.value === "error" ? (
                <p class="wizard-pairing__error">{error.value}</p>
              ) : phase.value === "requesting" ? (
                <p class="wizard-pairing__waiting">{s.requesting}</p>
              ) : (
                <div class="wizard-pairing__display">
                  <div class="wizard-pairing__code-row">
                    <HkBadge variant={ttl.value > 60 ? "info" : "warning"}>
                      {Math.floor(ttl.value / 60)}:
                      {String(ttl.value % 60).padStart(2, "0")}
                    </HkBadge>
                    <HkButton
                      variant="ghost"
                      size="sm"
                      ariaLabel={s.copy}
                      onClick={() => void copyCode()}
                    >
                      <Copy size={14} />
                    </HkButton>
                    <span class="wizard-pairing__copied">
                      {copied.value ? s.copied : ""}
                    </span>
                  </div>
                  <div class="wizard-pairing__cells">
                    {code.value.split("").map((ch) => (
                      <span class="wizard-pairing__cell">{ch}</span>
                    ))}
                  </div>
                  <p class="wizard-pairing__hint">
                    {refreshed.value ? s.refreshed : s.waiting}
                  </p>
                </div>
              )}
            </>
          )}
        </section>
      );
    };
  },
});
