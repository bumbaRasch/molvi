import { listen } from "@tauri-apps/api/event";

// TOCTOU-safe event-listen lifecycle: cleanup may run before a listen() promise
// resolves (pane torn down mid-await). The disposed flag drops the just-resolved
// unlisten instead of leaking it; cleanup() unsubscribes all resolved listeners.
// Lifted from recognition.ts (3 call sites) + microphone.ts (inline copy) — one
// shared copy of the same disposed-guard dance.

export interface SafeListeners {
  on: <T>(event: string, cb: (payload: T) => void) => void;
  cleanup: () => void;
}

export function safeListeners(): SafeListeners {
  const unlistens: Array<() => void> = [];
  let disposed = false;
  return {
    on: <T>(event: string, cb: (payload: T) => void): void => {
      void (async () => {
        try {
          const fn = await listen<T>(event, (e) => cb(e.payload));
          if (disposed) fn(); // pane left during await — drop the just-resolved listener
          else unlistens.push(fn);
        } catch (e) {
          console.error(`${event} listen failed`, e); // metadata-only
        }
      })();
    },
    cleanup: (): void => { disposed = true; unlistens.forEach((u) => u()); },
  };
}
