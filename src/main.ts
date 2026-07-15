import { mount } from "svelte";
import App from "./App.svelte";
import { initializeTheme } from "$lib/theme";
import "./styles/global.css";

initializeTheme();

async function bootstrap() {
  if (import.meta.env.MODE === "scenarios") {
    await import("@wdio/tauri-plugin");
  }

  return mount(App, {
    target: document.getElementById("app")!,
  });
}

const app = bootstrap();

export default app;
