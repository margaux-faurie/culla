// The demo's apply log renderer against a golden file holding the text the core's renderer is
// pinned to (`crates/core/src/applylog/tests.rs`, `whole_log_for_a_mixed_plan`, inline for now).
// Follow-up: an integration test under crates/core/tests/ reads the same file, so the two cannot
// drift (`include_str!` is allowed only there).
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { type LogHeader, type LogOp, renderLog } from "../src/backend/demo-log";

const file = fileURLToPath(new URL("../testdata/apply-log/mixed.txt", import.meta.url));

const header: LogHeader = {
  version: "0.1.0",
  plan: 12,
  purpose: "apply",
  run: "first",
  started: "2026-11-28 09:14:03 +01:00",
  source: "E:\\DCIM\\100NIKON",
  destination: "D:\\Photos\\2026",
};

const op = (
  id: number,
  group: number,
  kind: LogOp["kind"],
  root: LogOp["root"],
  src: string[],
  dst: string[],
  rest: Partial<LogOp> = {},
): LogOp => ({
  id,
  group,
  action: root === "rejected" ? "reject" : "keep",
  kind,
  src,
  dst,
  root,
  state: "pending",
  reason: null,
  code: null,
  attempts: 0,
  hash: null,
  ...rest,
});

describe("golden apply log", () => {
  it("renders the core's mixed fixture byte for byte, from the shared golden file", () => {
    const ab = "ab".repeat(32);
    const one = "01".repeat(32);
    const two = "02".repeat(32);
    const cd = "cd".repeat(32);
    const ops: LogOp[] = [
      op(1, 1, "rename", "rejected", ["DSC_0001.JPG"], ["DSC_0001.JPG"], { state: "done" }),
      op(2, 1, "rename", "rejected", ["DSC_0001.NEF"], ["DSC_0001.NEF"], { state: "committed" }),
      op(3, 2, "xmove", "destination", ["DSC_0002.JPG"], ["DSC_0002.JPG"], {
        state: "done",
        attempts: 1,
        hash: ab,
      }),
      op(4, 3, "xmove", "destination", ["DSC_0003.JPG"], ["DSC_0003.JPG"], {
        state: "copied_source_kept",
        attempts: 1,
        hash: one,
      }),
      op(5, 3, "xmove", "destination", ["DSC_0003.NEF"], ["DSC_0003.NEF"], {
        state: "failed",
        reason: "verify_mismatch",
        attempts: 2,
        hash: two,
      }),
      op(6, 4, "copy", "destination", ["sub", "DSC_0004.JPG"], ["sub", "DSC_0004_1.JPG"], {
        state: "done",
        attempts: 1,
        hash: cd,
      }),
      op(7, 5, "rename", "rejected", ["DSC_0005.JPG"], ["DSC_0005.JPG"], {
        state: "failed",
        reason: "io",
        code: 1392,
      }),
      op(8, 6, "rename", "rejected", ["DSC_0006.JPG"], ["DSC_0006.JPG"], {
        state: "failed",
        reason: "lost",
      }),
      op(9, 7, "rename", "rejected", ["DSC_0007.JPG"], ["DSC_0007.JPG"]),
    ];
    const want = readFileSync(file, "utf8");
    expect(
      renderLog(header, ops, {
        end: { kind: "paused", reason: "diskFull" },
        ended: "2026-11-28 09:15:41 +01:00",
      }),
    ).toBe(want);
  });
});
