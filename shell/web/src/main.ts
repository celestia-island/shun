import { createApp } from "vue";

import { HToast, installHkTooltipBridge } from "@celestia-island/hikari";

import App from "./App";
import PassiveView from "./PassiveView";
// hikari theme foundation first, then the app token layer (theme.scss), so
// app-local rules (app.scss) win the cascade — mirrors webui's load order.
import "./styles/hikari.scss";
import "./theme.scss";
import "./app.scss";
import "./passive.scss";

// One bundle, two windows: the installer window mounts the wizard; the
// silent lane's passive progress window (created by the Rust side with
// index.html?window=passive) mounts the compact progress face instead.
const passive = new URLSearchParams(window.location.search).get("window") === "passive";

if (!passive) {
  // Document-level bridge: every element carrying a native `title` (the log
  // drawer toggle, HkAffixPicker's chips — hikari internals included) is
  // upgraded in place to the hikari bubble tooltip, with zero per-component
  // migration. Installing twice is idempotent.
  installHkTooltipBridge();
}

createApp(passive ? PassiveView : App).mount("#app");

// The shared toast viewport rides its own tiny app instance beside the
// window's root: the error face's copy action speaks through hikari's
// module-singleton toast state, and without a mounted viewport the push
// would be invisible (2026-11-11 copy-feedback wave — hikari's constant-
// label copy controls rely on the toast being mounted somewhere).
const toastHost = document.createElement("div");
toastHost.dataset.shunToastHost = "";
document.body.appendChild(toastHost);
createApp(HToast).mount(toastHost);
