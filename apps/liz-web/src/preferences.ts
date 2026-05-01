export type ThemePreference = "system" | "light" | "dark";
export type DensityPreference = "comfortable" | "compact";
export type MarkdownPreference = "plain" | "rendered";
export type ToolVerbosityPreference = "brief" | "detailed";

export interface Preferences {
  serverUrl: string;
  theme: ThemePreference;
  density: DensityPreference;
  markdown: MarkdownPreference;
  toolVerbosity: ToolVerbosityPreference;
  developerMode: boolean;
  browserInstanceId: string;
}

const storageKey = "liz-web.preferences.v1";

const defaultPreferences: Preferences = {
  serverUrl: "ws://127.0.0.1:8787",
  theme: "system",
  density: "comfortable",
  markdown: "rendered",
  toolVerbosity: "brief",
  developerMode: false,
  browserInstanceId: "",
};

const isPreferenceRecord = (value: unknown): value is Partial<Preferences> =>
  typeof value === "object" && value !== null;

const generateBrowserInstanceId = () => {
  if (globalThis.crypto?.randomUUID) {
    return globalThis.crypto.randomUUID();
  }

  return `browser-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
};

export const loadPreferences = (storage: Storage = localStorage): Preferences => {
  try {
    const raw = storage.getItem(storageKey);
    const parsed = raw ? JSON.parse(raw) : {};
    const merged = isPreferenceRecord(parsed)
      ? { ...defaultPreferences, ...parsed }
      : defaultPreferences;

    if (!merged.browserInstanceId) {
      merged.browserInstanceId = generateBrowserInstanceId();
      savePreferences(merged, storage);
    }

    return merged;
  } catch {
    const fallback = { ...defaultPreferences, browserInstanceId: generateBrowserInstanceId() };
    savePreferences(fallback, storage);
    return fallback;
  }
};

export const savePreferences = (preferences: Preferences, storage: Storage = localStorage) => {
  storage.setItem(storageKey, JSON.stringify(preferences));
};
