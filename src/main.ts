import { mount } from "svelte";
import App from "./App.svelte";
import "./styles/global.css";

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
