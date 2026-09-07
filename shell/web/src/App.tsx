import { defineComponent, onMounted, ref } from "vue";
import { Box, Monitor, Usb } from "lucide-vue-next";
import {
  HAlert,
  HButton,
  HCheckbox,
  HMarkdownRenderer,
  HProgressBar,
  HSelectionGrid,
  HStepFlow,
} from "@celestia-island/hikari";

import AppTitleBar from "./components/AppTitleBar";
import { invoke, listen, openDirectory } from "./tauri";

/**
 * Shun demo shell UI — an NSIS-style delivery wizard rendered entirely with
 * hikari components. Everything shown is generated from the shun
 * configuration declared in the shell crate's Cargo.toml
 * (`[package.metadata.shun]`) and served by the `get_config` command:
 * product identity, delivery modes, and the license agreement page
 * (rendered through HMarkdownRenderer).
 */

type Mode = "local" | "portable";
type StepKey = "mode" | "license" | "install" | "done";

interface ProductIdentity {
  name: string;
  version: string;
  publisher?: string;
  logo?: string;
}

interface ProgressEvent {
  step?: string;
  percent?: number | null;
}

interface DirDefaults {
  dir: string;
}

interface ShellView {
  product: ProductIdentity;
  modes: Mode[];
  flash: boolean;
}

const MODE_COPY: Record<Mode, { title: string; description: string; icon: typeof Monitor }> = {
  local: {
    title: "安装到本机",
    description: "NSIS 式注册：ARP 卸载条目、开始菜单快捷方式与卸载器。",
    icon: Monitor,
  },
  portable: {
    title: "便携模式",
    description: "绿色免注册：只写 .shun-portable 标记，数据全部就地存放。",
    icon: Usb,
  },
};

const HINTS: Record<Mode, string> = {
  local: "登记到系统「应用」列表，可从设置或本界面卸载。",
  portable: "写入 .shun-portable 标记；卸载即删除整个目录。",
};

const STEPS: { key: StepKey; label: string }[] = [
  { key: "mode", label: "交付方式" },
  { key: "license", label: "许可协议" },
  { key: "install", label: "安装" },
  { key: "done", label: "完成" },
];

export default defineComponent({
  name: "ShunDemoApp",
  setup() {
    const product = ref<ProductIdentity>({ name: "ShunDemo", version: "" });
    const modes = ref<Mode[]>(["local", "portable"]);
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
    const note = ref<{ text: string; kind: "ok" | "err" } | null>(null);

    async function refreshDefaults() {
      const defaults = await invoke<DirDefaults>("default_dir", {
        mode: mode.value,
      });
      dir.value = defaults.dir;
      hint.value = HINTS[mode.value];
    }

    onMounted(() => {
      invoke<ShellView>("get_config")
        .then((view) => {
          product.value = view.product;
          modes.value = view.modes;
          if (!view.modes.includes(mode.value)) {
            mode.value = view.modes[0] ?? "local";
          }
          return refreshDefaults();
        })
        .catch((err) => {
          hint.value = String(err);
        });
      fetch("/demo-license.md")
        .then((r) => r.text())
        .then((text) => {
          licenseText.value = text;
          licenseLoading.value = false;
        })
        .catch(() => {
          licenseText.value = "许可协议文本加载失败。";
          licenseLoading.value = false;
        });
      listen<ProgressEvent>("install-progress", (payload) => {
        if (payload.step) flowStep.value = payload.step;
      });
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
      const picked = await openDirectory("选择安装位置");
      if (picked) dir.value = picked;
    }

    function go(key: StepKey) {
      step.value = key;
      if (key === "install" && !installed.value) {
        void install();
      }
    }

    async function install() {
      running.value = true;
      installFailed.value = false;
      flowStep.value = "正在准备安装…";
      try {
        await invoke("start_install", {
          mode: mode.value,
          dir: dir.value.trim(),
        });
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
      if (running.value) return;
      running.value = true;
      try {
        await invoke("uninstall_demo", {
          mode: mode.value,
          dir: dir.value.trim(),
        });
        installed.value = false;
        showNote(`✔ 已卸载：${dir.value.trim()}`);
        step.value = "mode";
      } catch (err) {
        showNote(String(err), "err");
      } finally {
        running.value = false;
      }
    }

    let noteTimer: number | undefined;
    function showNote(text: string, kind: "ok" | "err" = "ok") {
      note.value = { text, kind };
      window.clearTimeout(noteTimer);
      noteTimer = window.setTimeout(() => {
        note.value = null;
      }, 4000);
    }

    return () => {
      const modeItems = modes.value.map((id) => ({ id, ...MODE_COPY[id] }));

      return (
        <>
          <AppTitleBar icon="/logo.webp" title="Shun Demo Shell" showMaximize={false} />
          <main class="installer">
            <HStepFlow steps={STEPS} modelValue={step.value}>
              {{
                mode: () => (
                  <section class="wizard-pane">
                    <h1>选择 {product.value.name} 的交付方式</h1>
                    <p class="wizard-sub">
                      {product.value.version} · 由 shun 安装流驱动
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
                        安装位置
                      </label>
                      <div class="installer__row">
                        <input
                          id="dir-input"
                          type="text"
                          spellcheck={false}
                          v-model={dir.value}
                          disabled={running.value}
                        />
                        <HButton variant="ghost" disabled={running.value} onClick={browse}>
                          浏览…
                        </HButton>
                      </div>
                      <p class="installer__hint">{hint.value}</p>
                    </section>
                  </section>
                ),
                license: () => (
                  <section class="wizard-pane">
                    <div class="license-box">
                      <HMarkdownRenderer content={licenseText.value} loading={licenseLoading.value} />
                    </div>
                    <HCheckbox
                      modelValue={agreed.value}
                      label="我已阅读并同意上述许可协议"
                      onUpdate:modelValue={(v: boolean) => (agreed.value = v)}
                    />
                  </section>
                ),
                install: () => (
                  <section class="wizard-pane">
                    {installFailed.value ? (
                      <HAlert variant="error" title="安装失败" message={failMessage.value} />
                    ) : (
                      <>
                        <HProgressBar status={installed.value ? "done" : "loading"} size="md" />
                        <p class="installer__step">{flowStep.value}</p>
                      </>
                    )}
                  </section>
                ),
                done: () => (
                  <section class="wizard-pane wizard-done">
                    <p class="wizard-done__title">✔ 安装完成</p>
                    <p class="wizard-done__path">{dir.value.trim()}</p>
                    <p class="installer__hint">
                      {mode.value === "portable"
                        ? "入口点位于 bin/shun-demo.cmd；本副本未写入任何注册表项。"
                        : "已登记到系统「应用」列表，可从设置或下方按钮卸载。"}
                    </p>
                  </section>
                ),
              }}
            </HStepFlow>

            {note.value && (
              <HAlert
                variant={note.value.kind === "err" ? "error" : "success"}
                message={note.value.text}
                banner
              />
            )}

            <footer class="installer__footer">
              <div>
                {step.value === "install" && running.value && flowStep.value && (
                  <span class="wizard-live">{flowStep.value}</span>
                )}
              </div>
              <div class="installer__nav">
                {step.value === "mode" && (
                  <HButton variant="primary" size="lg" onClick={() => go("license")}>
                    下一步
                  </HButton>
                )}
                {step.value === "license" && (
                  <>
                    <HButton variant="ghost" onClick={() => go("mode")}>
                      上一步
                    </HButton>
                    <HButton
                      variant="primary"
                      size="lg"
                      disabled={!agreed.value}
                      onClick={() => go("install")}
                    >
                      同意并安装
                    </HButton>
                  </>
                )}
                {step.value === "install" && installFailed.value && (
                  <HButton variant="primary" onClick={() => go("license")}>
                    返回
                  </HButton>
                )}
                {step.value === "done" && (
                  <>
                    <HButton variant="ghost" onClick={remove} disabled={running.value}>
                      卸载
                    </HButton>
                    <HButton variant="primary" size="lg" onClick={() => currentWindow().close()}>
                      完成
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
