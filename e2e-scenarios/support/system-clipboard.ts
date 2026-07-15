import { execFileSync } from "node:child_process";

interface ClipboardExecOptions {
  encoding: "utf8";
  timeout: number;
  maxBuffer: number;
  windowsHide: boolean;
}

export type ClipboardExecutor = (
  command: string,
  args: string[],
  options: ClipboardExecOptions,
) => string;

interface ClipboardCommand {
  command: string;
  args: string[];
}

export function clipboardCommand(platform: NodeJS.Platform): ClipboardCommand {
  if (platform === "darwin") return { command: "pbpaste", args: [] };
  if (platform === "win32") {
    return {
      command: "powershell.exe",
      args: ["-NoProfile", "-NonInteractive", "-Command", "Get-Clipboard -Raw"],
    };
  }
  if (platform === "linux") {
    return { command: "xclip", args: ["-selection", "clipboard", "-out"] };
  }
  throw new Error(`The system clipboard is unsupported on ${platform}.`);
}

const executeClipboard: ClipboardExecutor = (command, args, options) =>
  execFileSync(command, args, options);

export function readSystemClipboard(
  runCommand: ClipboardExecutor = executeClipboard,
  platform: NodeJS.Platform = process.platform,
): string {
  const invocation = clipboardCommand(platform);
  try {
    const value = runCommand(invocation.command, invocation.args, {
      encoding: "utf8",
      timeout: 10_000,
      maxBuffer: 1024 * 1024,
      windowsHide: true,
    });
    return value.replace(/[\r\n]+$/u, "");
  } catch {
    throw new Error(`The system clipboard could not be read on ${platform}.`);
  }
}
