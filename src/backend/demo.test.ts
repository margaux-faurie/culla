import { describe, expect, it } from "vitest";
import { createBackend } from "./demo";

describe("demo backend", () => {
  it("reports the demo version", async () => {
    await expect(createBackend().version()).resolves.toBe("demo");
  });
});
