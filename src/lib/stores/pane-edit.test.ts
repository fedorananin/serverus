import { beforeEach, describe, expect, it, vi } from "vitest";

import type { RemoteEntry } from "$lib/api";
import { PaneController } from "$lib/stores/pane.svelte";

const apiMocks = vi.hoisted(() => {
  type Result =
    | { status: "ok"; data: null }
    | { status: "error"; error: { code: string; message: string } };
  return {
    localEditOpen: vi.fn(async (): Promise<Result> => ({ status: "ok", data: null })),
    remoteEditOpen: vi.fn(async (): Promise<Result> => ({ status: "ok", data: null })),
  };
});

vi.mock("$lib/api", () => ({
  commands: apiMocks,
  errorMessage: (error: unknown) => String(error),
  unwrap: async (
    promise: Promise<
      | { status: "ok"; data: unknown }
      | { status: "error"; error: { code: string; message: string } }
    >,
  ) => {
    const result = await promise;
    if (result.status === "error") throw result.error;
    return result.data;
  },
}));

function file(path: string): RemoteEntry {
  return {
    name: path.split("/").pop()!,
    path,
    is_dir: false,
    is_symlink: false,
    size: 1,
    mtime: 100,
    permissions: 0o644,
  };
}

describe("PaneController.edit", () => {
  beforeEach(() => {
    apiMocks.localEditOpen.mockClear();
    apiMocks.remoteEditOpen.mockClear();
  });

  it("opens local files in place", async () => {
    await new PaneController("local", null, false).edit(file("/Users/me/notes.txt"));

    expect(apiMocks.localEditOpen).toHaveBeenCalledWith("/Users/me/notes.txt");
    expect(apiMocks.remoteEditOpen).not.toHaveBeenCalled();
  });

  it("opens remote files through the session's edit cache", async () => {
    await new PaneController("remote", "session-a", false).edit(file("/srv/app.conf"));

    expect(apiMocks.remoteEditOpen).toHaveBeenCalledWith("session-a", "/srv/app.conf");
    expect(apiMocks.localEditOpen).not.toHaveBeenCalled();
  });

  it("surfaces the backend's refusal", async () => {
    apiMocks.localEditOpen.mockResolvedValueOnce({
      status: "error",
      error: { code: "other", message: "folders cannot be opened in an editor" },
    });

    await expect(
      new PaneController("local", null, false).edit(file("/Users/me/Thing.app")),
    ).rejects.toMatchObject({ message: "folders cannot be opened in an editor" });
  });
});
