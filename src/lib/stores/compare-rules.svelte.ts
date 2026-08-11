// Comparison rules for one session: mtime participates only when uploads can
// preserve it. S3 never can; FTP only when the server advertises MFMT — the
// backend's connect-time FEAT probe knows (`remote_preserves_mtime`). Without
// this, every file uploaded to such a server counts as "different" forever,
// so matching falls back to size-only. FTP's LIST mtime is additionally
// real-but-coarse (minutes, or date-only past ~6 months), so FTP always
// compares mtime at listing precision.

import { commands, unwrap } from "$lib/api";
import type { DirectoryComparisonOptions } from "$lib/directory-comparison";

export class CompareRulesController {
  /** Seeded per protocol, refined by the backend once it answers. */
  private preservesMtime = $state(true);
  private readonly coarseRemoteMtime: boolean;

  constructor(sessionId: string, isS3: boolean, isFtp: boolean) {
    this.preservesMtime = !isS3;
    this.coarseRemoteMtime = isFtp;
    void unwrap(commands.remotePreservesMtime(sessionId))
      .then((value) => (this.preservesMtime = value))
      .catch(() => {
        // Session already gone, or a legacy backend — keep the protocol seed.
      });
  }

  get options(): Required<DirectoryComparisonOptions> {
    return {
      ignoreMtime: !this.preservesMtime,
      coarseRemoteMtime: this.coarseRemoteMtime,
    };
  }
}
