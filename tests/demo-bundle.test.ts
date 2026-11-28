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
  assets: string[];
}

async function bundled(mode: "demo" | "desktop"): Promise<Bundle> {
  const result = await build({ root, mode, logLevel: "silent", build: { write: false } });
  const outputs = Array.isArray(result) ? result : [result];
  const bundle: Bundle = { code: "", moduleIds: [], assets: [] };
  for (const output of outputs) {
    if ("output" in output) {
      for (const file of output.output) {
        if (file.type === "chunk") {
          bundle.code += file.code;
          bundle.moduleIds.push(...file.moduleIds.map((id) => id.replaceAll("\\", "/")));
        } else {
          bundle.assets.push(file.fileName);
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

// SPEC §2 (28 Nov 2026): the demo site carries the download and install page, built from
// docs/install.md; the desktop app does not.
const installModules = (ids: string[]) =>
  ids.filter((id) => id.includes("/src/install/") || id.includes("/docs/install.md"));

describe("download and install page", () => {
  it("is built into the demo as install.html", { timeout: 60_000 }, async () => {
    const demo = await bundled("demo");
    expect(demo.assets).toContain("install.html");
    expect(installModules(demo.moduleIds).length).toBeGreaterThan(0);
  });

  it("stays out of the desktop build", { timeout: 60_000 }, async () => {
    const desktop = await bundled("desktop");
    expect(desktop.assets).not.toContain("install.html");
    expect(installModules(desktop.moduleIds)).toEqual([]);
  });
});
