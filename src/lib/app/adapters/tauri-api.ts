import { commands, unwrap } from "$lib/api";
import type {
  AppApi,
  TransfersApi,
  VaultActivityApi,
} from "$lib/app/contracts/api";

export class TauriAppApi implements AppApi {
  readonly transfers: TransfersApi = {
    list: () => unwrap(commands.transferList()),
    upload: async (sessionId, localPaths, remoteDir) => {
      await unwrap(commands.transferUpload(sessionId, localPaths, remoteDir));
    },
    download: async (sessionId, remotePaths, localDir) => {
      await unwrap(commands.transferDownload(sessionId, remotePaths, localDir));
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
    pauseAll: async (runtimeContextId, sessionId) => {
      await unwrap(commands.transferPauseAll(runtimeContextId, sessionId));
    },
    resumeAll: async (runtimeContextId, sessionId) => {
      await unwrap(commands.transferResumeAll(runtimeContextId, sessionId));
    },
    cancelAll: async (runtimeContextId, sessionId) => {
      await unwrap(commands.transferCancelAll(runtimeContextId, sessionId));
    },
    clearFinished: async (runtimeContextId, sessionId) => {
      await unwrap(commands.transferClearFinished(runtimeContextId, sessionId));
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
