import { beforeEach, expect, it, vi } from "vitest";
import { TauriAppApi } from "./adapters/tauri-api";

const commandMocks = vi.hoisted(() => ({
  transferUpload: vi.fn(async () => ({ status: "ok", data: null })),
  transferDownload: vi.fn(async () => ({ status: "ok", data: null })),
}));

vi.mock("$lib/api", () => ({
  commands: commandMocks,
  unwrap: async (promise: Promise<{ status: "ok"; data: unknown }>) => {
    const result = await promise;
    return result.data;
  },
}));

beforeEach(() => {
  commandMocks.transferUpload.mockClear();
  commandMocks.transferDownload.mockClear();
});

it("delegates upload and download to generated Tauri commands", async () => {
  const api = new TauriAppApi();

  await api.transfers.upload("session-a", ["/local/a.txt", "/local/b.txt"], "/remote");
  await api.transfers.download("session-a", ["/remote/b.txt"], "/local");

  expect(commandMocks.transferUpload).toHaveBeenCalledWith(
    "session-a",
    ["/local/a.txt", "/local/b.txt"],
    "/remote",
  );
  expect(commandMocks.transferDownload).toHaveBeenCalledWith(
    "session-a",
    ["/remote/b.txt"],
    "/local",
  );
});
