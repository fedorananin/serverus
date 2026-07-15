import { commands, unwrap } from "$lib/api";
import type {
  AppApi,
  TransfersApi,
  VaultActivityApi,
} from "$lib/app/contracts/api";

export class TauriAppApi implements AppApi {
  readonly transfers: TransfersApi = {
    list: () => unwrap(commands.transferList()),
    upload: async (sessionId, localPath, remoteDir) => {
      await unwrap(commands.transferUpload(sessionId, localPath, remoteDir));
    },
    download: async (sessionId, remotePath, localDir) => {
      await unwrap(commands.transferDownload(sessionId, remotePath, localDir));
    },
    pause: async (id) => {
      await unwrap(commands.transferPause(id));
    },
    retry: async (id) => {
      await unwrap(commands.transferRetry(id));
    },
    resume: async (id) => {
      await unwrap(commands.transferResume(id));
    },
    cancel: async (id) => {
      await unwrap(commands.transferCancel(id));
    },
    pauseAll: async (runtimeContextId) => {
      await unwrap(commands.transferPauseAll(runtimeContextId));
    },
    resumeAll: async (runtimeContextId) => {
      await unwrap(commands.transferResumeAll(runtimeContextId));
    },
    cancelAll: async (runtimeContextId) => {
      await unwrap(commands.transferCancelAll(runtimeContextId));
    },
    clearFinished: async (runtimeContextId) => {
      await unwrap(commands.transferClearFinished(runtimeContextId));
    },
    resolve: async (sessionId, id, action, applyToAll) => {
      await unwrap(commands.transferResolve(sessionId, id, action, applyToAll));
    },
  };

  readonly vault: VaultActivityApi = {
    touchActivity: async () => {
      await unwrap(commands.vaultTouchActivity());
    },
  };
}
