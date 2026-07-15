// @vitest-environment jsdom

import { fireEvent, render, screen } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";
import MainScreen from "./MainScreen.svelte";

const vaultMock = vi.hoisted(() => ({
  screen: "unlock" as "unlock" | "main",
  data: null,
}));

const tabsMock = vi.hoisted(() => ({
  tabs: [{ id: "tab-a", connectionId: "connection-a" }],
  activeId: "tab-a" as string | null,
  active: { id: "tab-a", connectionId: "connection-a" } as {
    id: string;
    connectionId: string;
  } | null,
  close: vi.fn(),
  open: vi.fn(),
  activateIndex: vi.fn(),
}));

const modelMock = vi.hoisted(() => ({
  transfers: { init: vi.fn() },
  api: { vault: { touchActivity: vi.fn(async () => {}) } },
  eventSource: {
    remoteEdit: { listenUploaded: vi.fn(async () => () => {}) },
  },
}));

vi.mock("$lib/stores/vault.svelte", () => ({ vault: vaultMock }));
vi.mock("$lib/stores/tabs.svelte", () => ({ tabs: tabsMock }));
vi.mock("$lib/app/model.svelte", () => ({ useAppModel: () => modelMock }));
vi.mock("$lib/platform", () => ({
  isMod: (event: KeyboardEvent) => event.metaKey || event.ctrlKey,
}));

vi.mock("$lib/components/Sidebar.svelte", async () => import("../test/Stub.svelte"));
vi.mock("$lib/components/SettingsDialog.svelte", async () => import("../test/Stub.svelte"));
vi.mock("$lib/components/SessionView.svelte", async () => import("../test/Stub.svelte"));
vi.mock("$lib/components/SessionTabBar.svelte", async () => import("../test/Stub.svelte"));
vi.mock("$lib/components/HostKeyDialog.svelte", async () => import("../test/Stub.svelte"));
vi.mock("$lib/components/TransferQueue.svelte", async () => import("../test/Stub.svelte"));
vi.mock("$lib/components/ConflictDialog.svelte", async () => import("../test/Stub.svelte"));
vi.mock("$lib/components/DragGhost.svelte", async () => import("../test/Stub.svelte"));
vi.mock("$lib/components/Toasts.svelte", async () => ({
  ...(await import("../test/Stub.svelte")),
  showToast: vi.fn(),
}));

describe("MainScreen lock boundary", () => {
  beforeEach(() => {
    vaultMock.screen = "unlock";
    tabsMock.close.mockReset();
    tabsMock.open.mockReset();
    tabsMock.activateIndex.mockReset();
    modelMock.transfers.init.mockReset();
    modelMock.api.vault.touchActivity.mockClear();
  });

  it("makes the mounted session UI inert and ignores input while locked", async () => {
    render(MainScreen);

    const main = screen.getByTestId("main-screen");
    expect(main).toHaveProperty("inert", true);
    expect(main).toHaveAttribute("aria-hidden", "true");

    await fireEvent.keyDown(window, { key: "w", ctrlKey: true });
    await fireEvent.mouseDown(window);

    expect(tabsMock.close).not.toHaveBeenCalled();
    expect(modelMock.api.vault.touchActivity).not.toHaveBeenCalled();
    expect(modelMock.transfers.init).not.toHaveBeenCalled();
  });

  it("keeps shortcuts and activity reporting enabled while unlocked", async () => {
    vaultMock.screen = "main";
    render(MainScreen);

    const main = screen.getByTestId("main-screen");
    expect(main).toHaveProperty("inert", false);
    expect(main).toHaveAttribute("aria-hidden", "false");

    await fireEvent.keyDown(window, { key: "w", ctrlKey: true });
    await fireEvent.mouseDown(window);

    expect(tabsMock.close).toHaveBeenCalledWith("tab-a");
    expect(modelMock.api.vault.touchActivity).toHaveBeenCalledOnce();
  });
});
