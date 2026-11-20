// @vitest-environment jsdom

import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it } from "vitest";

import type { Settings } from "$lib/api";
import EditorSection from "./EditorSection.svelte";

describe("EditorSection", () => {
  it("sets the editor for files without an app while the system default is on", async () => {
    const value: Settings["editor"] = { use_system_default: true, custom_app: null };
    render(EditorSection, { value });

    expect(screen.queryByLabelText("Application")).not.toBeInTheDocument();
    await fireEvent.input(screen.getByLabelText("Editor for files without an app"), {
      target: { value: "Visual Studio Code" },
    });
    expect(value.fallback_app).toBe("Visual Studio Code");
  });

  it("asks for one application for everything when the system default is off", async () => {
    const value: Settings["editor"] = {
      use_system_default: false,
      custom_app: null,
      fallback_app: "Zed",
    };
    render(EditorSection, { value });

    expect(screen.queryByLabelText("Editor for files without an app")).not.toBeInTheDocument();
    await fireEvent.input(screen.getByLabelText("Application"), { target: { value: "Zed" } });
    expect(value.custom_app).toBe("Zed");
    // The fallback is kept for when the system default comes back on.
    expect(value.fallback_app).toBe("Zed");
  });
});
