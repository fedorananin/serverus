<script lang="ts" module>
  interface Toast {
    id: number;
    text: string;
    error: boolean;
  }

  let toasts = $state<Toast[]>([]);
  let nextToastId = 1;

  export function showToast(text: string, error = false) {
    const toast: Toast = { id: nextToastId++, text, error };
    toasts.push(toast);
    setTimeout(() => {
      const idx = toasts.findIndex((t) => t.id === toast.id);
      if (idx !== -1) toasts.splice(idx, 1);
    }, error ? 15_000 : 10_000);
  }
</script>

{#if toasts.length > 0}
  <div class="toasts">
    {#each toasts as toast (toast.id)}
      <div class="toast" class:error={toast.error} role={toast.error ? "alert" : "status"}>
        {toast.text}
      </div>
    {/each}
  </div>
{/if}

<style>
  .toasts {
    position: fixed;
    bottom: 18px;
    right: 18px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    z-index: 500;
  }

  .toast {
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    padding: 7px 14px;
    font-size: 12px;
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.4);
    animation: slide-in 0.15s ease-out;
  }

  .toast.error {
    border-color: var(--danger);
  }

  @keyframes slide-in {
    from {
      transform: translateY(8px);
      opacity: 0;
    }
    to {
      transform: translateY(0);
      opacity: 1;
    }
  }
</style>
