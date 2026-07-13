// Typed API surface for the app: re-exports the generated bindings plus a
// small unwrap helper that converts the specta Result shape into throwing
// promises with a typed ApiError.

export { commands, events } from "./bindings";
export type * from "./bindings";

import type { ApiError } from "./bindings";

type SpectaResult<T> = { status: "ok"; data: T } | { status: "error"; error: ApiError };

/** Unwrap a specta command result, throwing the ApiError on failure. */
export async function unwrap<T>(promise: Promise<SpectaResult<T>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw result.error;
  return result.data;
}

export function isApiError(e: unknown): e is ApiError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}

/** Human-facing message from any thrown value. */
export function errorMessage(e: unknown): string {
  if (isApiError(e)) return e.message;
  if (e instanceof Error) return e.message;
  return String(e);
}
