import { createPinia } from "pinia";
import { createApp } from "vue";
import App from "./App.vue";
import { answerBackend } from "./platform/answerBackend";
import { audioBackend } from "./platform/audioBackend";
import { inTauri } from "./platform/overlayWindow";
import { createSession } from "./services/createSession";
import { SESSION_CONTROLLER } from "./services/sessionController";
import { useOverlaySettings } from "./stores/overlaySettings";
import { useSessionStore } from "./stores/session";
import "./styles/base.css";

if (!inTauri) document.documentElement.classList.add("is-browser");

// HTML5 drag and drop is on (`dragDropEnabled: false`) for the CV drop zone; a file dropped
// anywhere else must not make the webview navigate to it.
for (const type of ["dragover", "drop"]) {
  window.addEventListener(type, (event) => event.preventDefault());
}

const app = createApp(App);
const pinia = createPinia();
app.use(pinia);

// The UI talks to the pipeline only through SessionController commands and validated
// PipelineEvents: the demo pipeline in the browser; in the desktop app, Rust audio capture,
// question detection and answers.
const session = createSession({
  session: useSessionStore(pinia),
  settings: useOverlaySettings(pinia),
  backend: inTauri ? audioBackend : null,
  ...(inTauri ? { answers: answerBackend } : {}),
});
app.provide(SESSION_CONTROLLER, session.controller);

app.mount("#app");

import.meta.hot?.dispose(() => session.dispose());
