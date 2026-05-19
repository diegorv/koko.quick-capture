// Update preferences persisted via tauri-plugin-store. Svelte 5 runes
// expose the values reactively; `load()` is fire-and-forget on first
// access so UI can render the defaults during the round-trip and
// re-render once the store resolves.

import { Store } from "@tauri-apps/plugin-store";

export type ReleaseChannel = "stable" | "nightly";

interface UpdatePrefs {
  channel: ReleaseChannel;
  autoCheck: boolean;
  lastCheckedAt: number | null;
}

const STORE_FILE = "updates.json";
const KEY = "prefs";
const DEFAULTS: UpdatePrefs = {
  channel: "stable",
  autoCheck: true,
  lastCheckedAt: null,
};

let storePromise: Promise<Store> | null = null;
function getStore(): Promise<Store> {
  if (!storePromise) {
    storePromise = Store.load(STORE_FILE);
  }
  return storePromise;
}

let prefs = $state<UpdatePrefs>({ ...DEFAULTS });
let loaded = $state(false);

async function load() {
  if (loaded) return;
  try {
    const store = await getStore();
    const saved = (await store.get<UpdatePrefs>(KEY)) ?? null;
    if (saved) prefs = { ...DEFAULTS, ...saved };
  } catch (err) {
    console.error("updates store load failed", err);
  } finally {
    loaded = true;
  }
}

async function persist() {
  try {
    const store = await getStore();
    await store.set(KEY, $state.snapshot(prefs));
    await store.save();
  } catch (err) {
    console.error("updates store save failed", err);
  }
}

export const updatesStore = {
  get prefs() {
    if (!loaded) void load();
    return prefs;
  },
  get loaded() {
    return loaded;
  },
  load,
  update(partial: Partial<UpdatePrefs>) {
    prefs = { ...prefs, ...partial };
    void persist();
  },
};
