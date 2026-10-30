export interface NativeMenuItem {
  name: string;
  label: string;
  kind: "action";
}

// A failed platform close must retain its ComponentContent owner after plugin disposal.
const failedOwners: Set<ESObject> = new Set();
export class NativeMenuCleanup {
  private pending: Set<Promise<void>> = new Set();
  retire(
    owner: ESObject,
    opening: Promise<void>,
    close: () => Promise<void>,
    dispose: () => void,
  ): void {
    const operation = (async (): Promise<void> => {
      try {
        await opening.catch((): void => {});
        await close();
        dispose();
      } catch (error) {
        failedOwners.add(owner);
        console.error(`[NSH] menu retirement failed: ${String(error)}`);
      }
    })();
    this.pending.add(operation);
    operation.finally((): void => {
      this.pending.delete(operation);
    });
  }
  async settled(): Promise<void> {
    while (this.pending.size > 0) {
      await Promise.all(Array.from(this.pending));
    }
  }
}
