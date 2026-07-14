export interface Toast {
  id: number;
  text: string;
  error: boolean;
}

class ToastsStore {
  items = $state<Toast[]>([]);
  private nextId = 1;

  show(text: string, error = false) {
    const toast: Toast = { id: this.nextId++, text, error };
    this.items.push(toast);
    setTimeout(() => {
      const index = this.items.findIndex((item) => item.id === toast.id);
      if (index !== -1) this.items.splice(index, 1);
    }, error ? 6000 : 2500);
  }

  resetVaultContext() {
    this.items = [];
  }
}

export const toasts = new ToastsStore();

export function showToast(text: string, error = false) {
  toasts.show(text, error);
}
