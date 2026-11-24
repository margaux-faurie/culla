// ADR 0014: the demo bundle never includes @tauri-apps/*. Builds both modes in memory and looks
// for the Tauri IPC global, which every @tauri-apps/api call goes through.
import { fileURLToPath } from "node:url";
import { build } from "vite";
import { describe, expect, it } from "vitest";

const root = fileURLToPath(new URL("..", import.meta.url));
const TAURI_MARKER = "__TAURI_INTERNALS__";

interface Bundle {
  code: string;
  moduleIds: string[];
}

async function bundled(mode: "demo" | "desktop"): Promise<Bundle> {
  const result = await build({ root, mode, logLevel: "silent", build: { write: false } });
  const outputs = Array.isArray(result) ? result : [result];
  const bundle: Bundle = { code: "", moduleIds: [] };
  for (const output of outputs) {
    if ("output" in output) {
      for (const file of output.output) {
        if (file.type === "chunk") {
          bundle.code += file.code;
          bundle.moduleIds.push(...file.moduleIds.map((id) => id.replaceAll("\\", "/")));
        }
      }
    }
  }
  return bundle;
}

const fromTauriPackages = (ids: string[]) => ids.filter((id) => id.includes("/@tauri-apps/"));

describe("backend selection at build time", () => {
  it("keeps @tauri-apps out of the demo bundle", { timeout: 60_000 }, async () => {
    const demo = await bundled("demo");
    expect(demo.code.length).toBeGreaterThan(0);
    expect(fromTauriPackages(demo.moduleIds)).toEqual([]);
    expect(demo.code).not.toContain(TAURI_MARKER);
  });

  // Control: proves the check above can fail.
  it("links the Tauri backend in the desktop bundle", { timeout: 60_000 }, async () => {
    const desktop = await bundled("desktop");
    expect(fromTauriPackages(desktop.moduleIds).length).toBeGreaterThan(0);
    expect(desktop.code).toContain(TAURI_MARKER);
  });
});
