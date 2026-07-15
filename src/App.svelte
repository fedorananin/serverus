<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import { tabs } from "$lib/stores/tabs.svelte";
  import { hostKey } from "$lib/stores/hostkey.svelte";
  import {
    createTauriAppModel,
    provideAppModel,
    wireAccessRevocation,
    wireContextRetirement,
  } from "$lib/app/model.svelte";
  import { commands, unwrap, type ThemePreference } from "$lib/api";
  import { setThemePreference } from "$lib/theme";
  import UnlockScreen from "./routes/UnlockScreen.svelte";
  import MainScreen from "./routes/MainScreen.svelte";

  const model = createTauriAppModel();
  provideAppModel(model);
  let nextRemoteEditNoticeId = 1;
  let remoteEditNotices = $state<Array<{ id: number; text: string; error: boolean }>>([]);
  let themedAccessGeneration = -1;
  let lastSavedTheme: ThemePreference | null = null;

  function showRemoteEditNotice(text: string, error: boolean) {
    const notice = { id: nextRemoteEditNoticeId++, text, error };
    remoteEditNotices.push(notice);
    window.setTimeout(() => {
      const index = remoteEditNotices.findIndex(({ id }) => id === notice.id);
      if (index !== -1) remoteEditNotices.splice(index, 1);
    }, error ? 15_000 : 10_000);
  }

  $effect(() => {
    void vault.init();
  });

  $effect(() => {
    const savedTheme = vault.data?.settings.appearance?.theme;
    if (!savedTheme) return;
    const accessGeneration = vault.accessGeneration;
    if (accessGeneration === themedAccessGeneration && savedTheme === lastSavedTheme) return;
    themedAccessGeneration = accessGeneration;
    lastSavedTheme = savedTheme;
    setThemePreference(savedTheme);
  });

  $effect(() => {
    let inFlight = false;
    const poll = async () => {
      if (inFlight || vault.screen !== "main") return;
      inFlight = true;
      try {
        for (const event of await unwrap(commands.remoteEditNotifications())) {
          const isError = Boolean(event.error);
          showRemoteEditNotice(
            isError
              ? `Upload of ${event.name} failed: ${event.error}`
              : `Uploaded ${event.name} ✓`,
            isError,
          );
        }
      } catch {
        // Lock/context retirement can race a poll; the next main-screen tick retries.
      } finally {
        inFlight = false;
      }
    };
    const timer = window.setInterval(() => void poll(), 1_000);
    void poll();
    return () => window.clearInterval(timer);
  });

  $effect(() => wireContextRetirement(vault, tabs, hostKey, model.transfers));
  $effect(() => wireAccessRevocation(vault, hostKey));

  // Keep the main screen (and its live terminals/sessions) mounted while
  // locked — SPEC §2.4: locking wipes keys, it does not kill sessions. The
  // opaque unlock overlay hides everything.
  const keepMainMounted = $derived(vault.screen === "main" || tabs.tabs.length > 0);
</script>

{#if keepMainMounted}
  <MainScreen />
{/if}

{#if vault.screen === "unlock"}
  <div class="unlock-overlay" data-testid="unlock-overlay">
    <UnlockScreen />
  </div>
{/if}

{#if vault.screen === "main" && remoteEditNotices.length > 0}
  <div class="remote-edit-notices">
    {#each remoteEditNotices as notice (notice.id)}
      <div class="remote-edit-notice" class:error={notice.error} role={notice.error ? "alert" : "status"}>
        {notice.text}
      </div>
    {/each}
  </div>
{/if}

<style>
  .remote-edit-notices {
    position: fixed;
    right: 18px;
    bottom: 18px;
    z-index: 510;
    display: grid;
    gap: 6px;
  }
  .remote-edit-notice {
    padding: 7px 14px;
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-float);
    font-size: 12px;
  }
  .remote-edit-notice.error { border-color: var(--danger); }
</style>
