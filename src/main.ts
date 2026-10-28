import { createPinia } from "pinia";
import { createApp } from "vue";
import App from "./App.vue";
import { audioBackend } from "./platform/audioBackend";
import { inTauri } from "./platform/overlayWindow";
import { createSession } from "./services/createSession";
import { SESSION_CONTROLLER } from "./services/sessionController";
import { useOverlaySettings } from "./stores/overlaySettings";
import { useSessionStore } from "./stores/session";
import "./styles/base.css";

if (!inTauri) document.documentElement.classList.add("is-browser");

const app = createApp(App);
const pinia = createPinia();
app.use(pinia);

// The UI talks to the pipeline only through SessionController commands and validated
// PipelineEvents: the demo pipeline in the browser, Rust audio capture in the desktop app.
const session = createSession({
  session: useSessionStore(pinia),
  settings: useOverlaySettings(pinia),
  backend: inTauri ? audioBackend : null,
});
app.provide(SESSION_CONTROLLER, session.controller);

app.mount("#app");

import.meta.hot?.dispose(() => session.dispose());
