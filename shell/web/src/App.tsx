import { defineComponent, onMounted, ref } from "vue";
import { Box, Monitor, Usb } from "lucide-vue-next";
import { HButton, HProgressBar, HSelectionGrid } from "@celestia-island/hikari";

import AppTitleBar from "./components/AppTitleBar";
import { invoke, listen, openDirectory } from "./tauri";

/**
 * Shun demo shell UI — everything shown is driven by the shun configuration
 * declared in the shell crate's Cargo.toml (`[package.metadata.shun]`) and
// served by the `get_config` command: product identity, delivery modes,
 * and the flash-target placeholder. Rendered with hikari components.
 */

type Mode = "local" | "portable";

interface ProductIdentity {
  name: string;
  version: string;
  publisher?: string;
  logo?: string;
}

interface ShellView {
  product: ProductIdentity;
  modes: Mode[];
  flash: boolean;
}

interface ProgressEvent {
  step?: string;
  percent?: number | null;
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

export default defineComponent({
  name: "ShunDemoApp",
  setup() {
    const product = ref<ProductIdentity>({ name: "ShunDemo", version: "" });
    const modes = ref<Mode[]>(["local", "portable"]);
    const flashDeclared = ref(false);
    const mode = ref<Mode>("local");
    const dir = ref("");
    const hint = ref("");
    const running = ref(false);
    const done = ref(false);
    const step = ref("");
    const note = ref("");
    const noteKind = ref<"" | "ok" | "err">("");

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
          flashDeclared.value = view.flash;
          return refreshDefaults();
        })
        .catch((err) => {
          hint.value = String(err);
        });
      listen<ProgressEvent>("install-progress", (payload) => {
        if (payload.step) step.value = payload.step;
      });
    });

    async function selectMode(id: string | number | boolean | undefined) {
      if (running.value || done.value) return;
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

    async function start() {
      if (running.value) return;
      running.value = true;
      done.value = false;
      note.value = "";
      step.value = "正在准备安装…";
      try {
        await invoke("start_install", {
          mode: mode.value,
          dir: dir.value.trim(),
        });
        done.value = true;
        note.value = `✔ 安装完成：${dir.value.trim()}`;
        noteKind.value = "ok";
      } catch (err) {
        note.value = String(err);
        noteKind.value = "err";
      } finally {
        running.value = false;
      }
    }

    async function remove() {
      if (running.value || !done.value) return;
      running.value = true;
      note.value = "";
      try {
        await invoke("uninstall_demo", {
          mode: mode.value,
          dir: dir.value.trim(),
        });
        note.value = `✔ 已卸载：${dir.value.trim()}`;
        noteKind.value = "ok";
        step.value = "";
        done.value = false;
      } catch (err) {
        note.value = String(err);
        noteKind.value = "err";
      } finally {
        running.value = false;
      }
    }

    return () => {
      const items = modes.value.map((id) => ({
        id,
        ...MODE_COPY[id],
      }));
      if (flashDeclared.value) {
        items.push({
          id: "flash",
          title: "镜像烧写",
          description: "块设备写入与校验 —— 随 evernight 烧写器接入。",
          icon: Box,
        });
      }

      return (
        <>
          <AppTitleBar icon="/logo.webp" title="Shun Demo Shell" showMaximize={false} />
          <main class="installer">
            <section class="installer__hero">
              <h1>选择 {product.value.name} 的交付方式</h1>
              <p>
                {product.value.version} · 由 shun 安装流驱动的交付演示
                {product.value.publisher ? ` · ${product.value.publisher}` : ""}
              </p>
            </section>

            <HSelectionGrid
              items={items}
              selectedId={mode.value}
              columns={items.length > 2 ? 3 : 2}
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

            {(running.value || (done.value && step.value)) && (
              <section class="installer__progress">
                <HProgressBar status={done.value ? "done" : "loading"} size="md" />
                {step.value && <p class="installer__step">{step.value}</p>}
              </section>
            )}

            <footer class="installer__footer">
              {note.value && (
                <p class={`installer__note installer__note--${noteKind.value || "muted"}`}>
                  {note.value}
                </p>
              )}
              {done.value ? (
                <HButton variant="danger" disabled={running.value} onClick={remove}>
                  卸载
                </HButton>
              ) : (
                <HButton
                  variant="primary"
                  size="lg"
                  disabled={running.value}
                  onClick={start}
                >
                  开始安装
                </HButton>
              )}
            </footer>
          </main>
        </>
      );
    };
  },
});
