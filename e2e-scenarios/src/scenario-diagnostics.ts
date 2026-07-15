export async function settleWithin(promise: Promise<unknown>, timeoutMs: number): Promise<boolean> {
  let timer: NodeJS.Timeout | undefined;
  const timeout = new Promise<false>((resolveTimeout) => {
    timer = setTimeout(() => resolveTimeout(false), timeoutMs);
  });
  try {
    return await Promise.race([promise.then(() => true, () => false), timeout]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}
