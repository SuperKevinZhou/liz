import { loadPreferences, savePreferences, type Preferences } from "../src/preferences";

class MemoryStorage implements Storage {
  private readonly items = new Map<string, string>();

  get length() {
    return this.items.size;
  }

  clear() {
    this.items.clear();
  }

  getItem(key: string) {
    return this.items.get(key) ?? null;
  }

  key(index: number) {
    return Array.from(this.items.keys())[index] ?? null;
  }

  removeItem(key: string) {
    this.items.delete(key);
  }

  setItem(key: string, value: string) {
    this.items.set(key, value);
  }
}

describe("preferences", () => {
  it("persists local console preferences", () => {
    const storage = new MemoryStorage();
    const initial = loadPreferences(storage);
    const preferences: Preferences = {
      ...initial,
      serverUrl: "ws://127.0.0.1:9000",
      density: "compact",
    };

    savePreferences(preferences, storage);

    expect(loadPreferences(storage)).toMatchObject({
      serverUrl: "ws://127.0.0.1:9000",
      density: "compact",
      browserInstanceId: initial.browserInstanceId,
    });
  });
});
