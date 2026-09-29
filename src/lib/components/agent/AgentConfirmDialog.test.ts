// @vitest-environment jsdom

import { fireEvent, render, screen } from "@testing-library/svelte";
import { expect, it, vi } from "vitest";
import { AgentStore } from "$lib/stores/agent.svelte";
import { inertAgentApi, inertAgentEvents } from "../../../test/agent-fakes";
import AgentConfirmDialog from "./AgentConfirmDialog.svelte";

const model = vi.hoisted(() => ({ agent: null as unknown }));
vi.mock("$lib/app/model.svelte", () => ({ useAppModel: () => model }));

it("shows the oldest request and sends the user's decision", async () => {
  const api = inertAgentApi();
  const respond = vi.spyOn(api, "respond");
  const store = new AgentStore(api, inertAgentEvents());
  model.agent = store;
  store.confirmations.push(
    { requestId: "r1", connectionId: "c1", server: "Prod/web", action: "Run a command", detail: "rm -rf /tmp/x" },
    { requestId: "r2", connectionId: "c1", server: "Prod/web", action: "Delete", detail: "/srv" },
  );
  render(AgentConfirmDialog);

  expect(screen.getByRole("dialog", { name: "AI agent request" })).toBeInTheDocument();
  expect(screen.getByText("rm -rf /tmp/x")).toBeInTheDocument();
  expect(screen.getByText("1 more request waiting")).toBeInTheDocument();

  await fireEvent.click(screen.getByRole("button", { name: "Allow once" }));
  expect(respond).toHaveBeenCalledWith("r1", { kind: "confirm", decision: "once" });
  expect(screen.getByText("/srv")).toBeInTheDocument();

  await fireEvent.keyDown(window, { key: "Escape" });
  expect(respond).toHaveBeenCalledWith("r2", { kind: "confirm", decision: "deny" });
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

it("takes focus away instead of giving it to a button", async () => {
  const store = new AgentStore(inertAgentApi(), inertAgentEvents());
  model.agent = store;
  const typing = document.createElement("textarea");
  document.body.append(typing);
  typing.focus();
  render(AgentConfirmDialog);
  store.confirmations.push({
    requestId: "r1",
    connectionId: "c1",
    server: "Prod/web",
    action: "Delete",
    detail: "/srv",
  });
  await vi.waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
  // Neither the terminal the user was typing in nor "Allow once" has focus.
  expect(document.activeElement).not.toBe(typing);
  expect(document.activeElement).not.toBe(screen.getByRole("button", { name: "Allow once" }));
  typing.remove();
});
