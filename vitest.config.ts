import { svelteTesting } from "@testing-library/svelte/vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [
    svelte({
      onwarn(warning, handler) {
        if (warning.code !== "state_referenced_locally") handler(warning);
      },
    }),
    svelteTesting(),
  ],
  resolve: {
    alias: {
      $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
    },
  },
  test: {
    setupFiles: ["./src/test/setup.ts"],
  },
});
