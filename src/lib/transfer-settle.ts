import type { TransferSummary } from "$lib/app/contracts/api";

/** The observed activity of one session's transfer queue. */
export interface QueueActivity {
  /** Paused and conflicted items count as busy — nothing has finished yet. */
  busy: boolean;
  finished: number;
}

export function queueActivity(summary: TransferSummary): QueueActivity {
  return {
    busy: summary.queued + summary.running > 0,
    finished: summary.done + summary.failed,
  };
}

/** Whether the queue just settled, i.e. the panes should be relisted.
 *
 * Settling is the busy → idle transition, plus the case where a transfer
 * finished so fast that no snapshot ever showed it queued or running — the
 * finished count grew while the queue looked idle the whole time. A shrinking
 * finished count (Clear finished, session teardown) is not a settle.
 */
export function queueSettled(previous: QueueActivity, current: QueueActivity): boolean {
  if (current.busy) return false;
  return previous.busy || current.finished > previous.finished;
}
