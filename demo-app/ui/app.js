      const invoke = (cmd) =>
        window.__TAURI__?.core?.invoke?.(cmd) ?? Promise.reject(new Error("no ipc"));
      const fill = (id, value) => {
        document.getElementById(id).textContent = value;
      };
      invoke("delivery_mode")
        .then((mode) => {
          fill("mode", mode === "portable" ? "便携模式（免注册表）" : "本机安装（含注册项）");
          const chip = document.getElementById("chip-mode");
          chip.textContent = mode === "portable" ? "portable" : "local";
          chip.classList.remove("placeholder");
        })
        .catch(() => fill("mode", "未知"));
      invoke("install_root").then((root) => fill("root", root)).catch(() => fill("root", "未知"));
      invoke("app_version")
        .then((version) => {
          const chip = document.getElementById("chip-version");
          chip.textContent = "v" + version;
          chip.classList.remove("placeholder");
        })
        .catch(() => {});
