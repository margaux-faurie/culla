import { describe, expect, it } from "vitest";
import { escaped, type LogHeader, type LogOp, logTime, renderLog } from "./demo-log";

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

describe("demo apply log (the core's applylog::render)", () => {
  it("renders the core's mixed fixture byte for byte", () => {
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
    const want = `Culla apply log
version      0.1.0
plan         12 (apply, first run)
started      2026-11-28 09:14:03 +01:00
source       E:\\DCIM\\100NIKON
destination  D:\\Photos\\2026
rejected     the _rejected folder in the source

alarms: a file is not where the journal says. Run chkdsk on the volume before anything else.
  op 8  group 6  reject  rename  source "DSC_0006.JPG" → rejected "DSC_0006.JPG"  failed (lost)

operations, in plan order
  op 1  group 1  reject  rename  source "DSC_0001.JPG" → rejected "DSC_0001.JPG"  done
  op 2  group 1  reject  rename  source "DSC_0001.NEF" → rejected "DSC_0001.NEF"  committed
  op 3  group 2  keep  xmove  source "DSC_0002.JPG" → destination "DSC_0002.JPG"  done  blake3 ${ab}
  op 4  group 3  keep  xmove  source "DSC_0003.JPG" → destination "DSC_0003.JPG"  copied_source_kept  blake3 ${one}
  op 5  group 3  keep  xmove  source "DSC_0003.NEF" → destination "DSC_0003.NEF"  failed (verify_mismatch)  attempts 2  blake3 ${two}
  op 6  group 4  keep  copy  source "sub\\DSC_0004.JPG" → destination "sub\\DSC_0004_1.JPG"  done  blake3 ${cd}
  op 7  group 5  reject  rename  source "DSC_0005.JPG" → rejected "DSC_0005.JPG"  failed (io 1392)
  op 8  group 6  reject  rename  source "DSC_0006.JPG" → rejected "DSC_0006.JPG"  failed (lost)
  op 9  group 7  reject  rename  source "DSC_0007.JPG" → rejected "DSC_0007.JPG"  pending

summary
  operations          9
  pending             1
  committed           1
  done                3
  copied_source_kept  1
  failed              3
  alarms              1
end          paused (the destination is full)
ended        2026-11-28 09:15:41 +01:00
`;
    expect(
      renderLog(header, ops, {
        end: { kind: "paused", reason: "diskFull" },
        ended: "2026-11-28 09:15:41 +01:00",
      }),
    ).toBe(want);
  });

  it("escapes names as the core does (threat model T6b)", () => {
    const log = renderLog(
      { ...header, source: "E:\\\u202eMICD", destination: null },
      [
        op(1, 1, "rename", "rejected", ["dir", "u{a}x"], ["dir\nx"]),
        op(2, 2, "rename", "rejected", ["d\u202dir", 'x"'], ['"y']),
      ],
      { end: { kind: "cancelled" }, ended: "09:01" },
    );
    expect(log).toContain('source "dir\\u\\u{7b}a}x" → rejected "dir\\u{a}x"');
    expect(log).toContain('source "d\\u{202d}ir\\x\\u{22}" → rejected "\\u{22}y"');
    expect(log).toContain("source       E:\\\\u{202e}MICD\n");
    expect(log).toContain("destination  none\n");
    expect(log).toContain("end          cancelled\n");
    expect(log).not.toContain("alarms");
  });

  it("escapes control, bidi and invisible characters, and only those", () => {
    expect(escaped("a\u001b[31mb", () => false)).toBe("a\\u{1b}[31mb");
    expect(escaped("é 📷 ok", () => false)).toBe("é 📷 ok");
    expect(escaped("\u{e0041}", () => false)).toBe("\\u{e0041}");
  });

  it("formats the time as the adapter does, with the offset", () => {
    expect(logTime(new Date(2026, 10, 28, 9, 4, 3))).toMatch(/^2026-11-28 09:04:03 [+-]\d\d:\d\d$/);
  });
});
