// @vitest-environment jsdom

import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";

import DirectoryComparisonBar from "./DirectoryComparisonBar.svelte";

const summary = {
  matching: 7,
  different: 2,
  localOnly: 3,
  remoteOnly: 1,
};

describe("DirectoryComparisonBar", () => {
  it("starts as a compact inactive comparison control", async () => {
    const ontoggle = vi.fn();
    render(DirectoryComparisonBar, {
      active: false,
      summary,
      differencesOnly: false,
      ontoggle,
      onfilterchange: vi.fn(),
    });

    const button = screen.getByRole("button", { name: "Compare Folders" });
    expect(button).toHaveAttribute("aria-pressed", "false");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();

    await fireEvent.click(button);
    expect(ontoggle).toHaveBeenCalledOnce();
  });

  it("announces every result category without relying on color alone", () => {
    render(DirectoryComparisonBar, {
      active: true,
      summary,
      differencesOnly: false,
      ontoggle: vi.fn(),
      onfilterchange: vi.fn(),
    });

    expect(screen.getByRole("button", { name: "Stop Comparing Folders" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      "3 Local Only 2 Different 1 Remote Only 7 Same Metadata",
    );
    expect(screen.queryByText("Current Folders · Metadata Only")).not.toBeInTheDocument();
  });

  it("exposes a labelled differences-only filter", async () => {
    const onfilterchange = vi.fn();
    render(DirectoryComparisonBar, {
      active: true,
      summary,
      differencesOnly: false,
      ontoggle: vi.fn(),
      onfilterchange,
    });

    await fireEvent.click(screen.getByRole("checkbox", { name: "Differences Only" }));

    expect(onfilterchange).toHaveBeenCalledWith(true);
  });
});
