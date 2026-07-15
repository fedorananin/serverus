import { errorMessage } from "$lib/api";

export interface ClipboardNote {
  text: string;
  error: boolean;
}

export async function copyPublicUrl(url: string): Promise<ClipboardNote> {
  try {
    await navigator.clipboard.writeText(url);
    return { text: "Public URL copied", error: false };
  } catch (error) {
    return { text: `Copy public URL failed: ${errorMessage(error)}`, error: true };
  }
}
