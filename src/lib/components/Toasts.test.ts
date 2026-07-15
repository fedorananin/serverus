// @vitest-environment jsdom

import { render, screen } from "@testing-library/svelte";
import { describe, expect, it } from "vitest";

import Toasts, { showToast } from "./Toasts.svelte";

describe("Toasts accessibility", () => {
  it("announces successful and failed remote-edit results with distinct roles", async () => {
    render(Toasts);

    showToast("Uploaded ✓");
    expect(await screen.findByRole("status")).toHaveTextContent("Uploaded ✓");

    showToast("Promotion failed", true);
    expect(await screen.findByRole("alert")).toHaveTextContent("Promotion failed");
  });
});
