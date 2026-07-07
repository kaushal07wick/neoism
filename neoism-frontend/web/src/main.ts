import { App } from "./app/App";
import "./styles/app.css";
import "./styles/mobile.css";

function bootstrap(): void {
  const root = document.getElementById("root");
  if (!root) {
    throw new Error("#root element missing in index.html");
  }
  new App(root);
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", bootstrap, { once: true });
} else {
  bootstrap();
}
