import { defineComponent, onMounted, ref } from "vue";
import { HardDrive, Monitor } from "lucide-vue-next";
import { HButton, HProgressBar, HSelectionGrid } from "@celestia-island/hikari";

import AppTitleBar from "./components/AppTitleBar";
import { invoke, listen, openDirectory } from "./tauri";

/**
 * Shun demo shell UI — two delivery modes over the shun install flow, with
 * hikari components (AppTitleBar chrome, HSelectionGrid mode picker,
 * HProgressBar fed by the flow's real progress events).
 */

type Mode = "local" | "portable";

const MODE_ITEMS = [
  {
    id: "local",
    title: "安装到本机",
    description: "NSIS 式注册：ARP 卸载条目、开始菜单快捷方式与卸载器。",
    badge: "推荐",
    icon: Monitor,
  },
  {
    id: "portable",
    title: "便携模式",
    description: "绿色免注册：只写 .shun-portable 标记，数据全部就地存放。",
    icon: HardDrive,
  },
];

const HINTS: Record<Mode, string> = {
  local: "登记到系统「应用」列表，可从设置或本界面卸载。",
  portable: "写入 .shun-portable 标记；卸载即删除整个目录。",
};

interface DirDefaults {
  dir: string;
}

interface ProgressEvent {
  step?: string;
  percent?: number | null;
}

export default defineComponent({
  name: "ShunDemoApp",
  setup() {
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
      refreshDefaults().catch((err) => {
        hint.value = String(err);
      });
      listen<ProgressEvent>("install-progress", (payload) => {
        if (payload.step) step.value = payload.step;
      });
    });

    async function selectMode(id: string | number | boolean | undefined) {
      if (running.value) return;
      mode.value = (id as Mode) ?? "local";
      done.value = false;
      note.value = "";
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

    return () => (
      <>
        <AppTitleBar icon="/logo.webp" title="Shun Demo Shell" showMaximize={false} />
        <main class="installer">
          <section class="installer__hero">
            <h1>选择 ShunDemo 的交付方式</h1>
            <p>由 shun 安装流驱动的双模式交付演示。</p>
          </section>

          <HSelectionGrid
            items={MODE_ITEMS}
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

          {(running.value || (done.value && step.value)) && (
            <section class="installer__progress">
              <HProgressBar
                status={done.value ? "done" : "loading"}
                size="md"
              />
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
  },
});
