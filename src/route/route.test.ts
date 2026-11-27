import { describe, expect, it } from "vitest";
import type { PhotoSummary } from "../backend/types";
import {
  filesOf,
  joinPath,
  keepVerb,
  keptFolder,
  photoFolder,
  rejectedFolder,
  rejectVerb,
  suffixes,
} from "./route";

const photo = (extra: Partial<PhotoSummary> = {}): PhotoSummary => ({
  id: "g1",
  name: "MGF_7273",
  folder: ["Barcelona"],
  primary: ".JPG",
  raw: ".NEF",
  sidecars: [
    { suffix: ".xmp", follows: "raw" },
    { suffix: ".JPG.xmp", follows: "primary" },
  ],
  cameraProtected: false,
  previewUrl: "",
  alt: "",
  ...extra,
});

describe("route paths", () => {
  it("joins display folders with backslashes", () => {
    expect(joinPath("E:\\2026", ["Barcelona"])).toBe("E:\\2026\\Barcelona");
    expect(joinPath("E:\\", ["2026"])).toBe("E:\\2026");
    expect(joinPath("E:\\2026", [])).toBe("E:\\2026");
  });

  it("mirrors subfolders for kept and rejected photos", () => {
    const p = photo();
    expect(photoFolder("E:\\2026", p)).toBe("E:\\2026\\Barcelona");
    expect(keptFolder("D:\\Selects", p)).toBe("D:\\Selects\\Barcelona");
    expect(rejectedFolder("E:\\2026", p)).toBe("E:\\2026\\_rejected\\Barcelona");
    expect(rejectedFolder("E:\\2026", photo({ folder: [] }))).toBe("E:\\2026\\_rejected");
  });

  it("moves the NEF and its sidecars with the JPEG when RAW follows", () => {
    const files = filesOf(photo(), true);
    expect(files.going).toEqual([
      "MGF_7273.JPG",
      "MGF_7273.NEF",
      "MGF_7273.xmp",
      "MGF_7273.JPG.xmp",
    ]);
    expect(files.staying).toEqual([]);
    expect(suffixes(photo(), files.going)).toBe(".JPG .NEF .xmp .JPG.xmp");
  });

  it("leaves the NEF and the sidecars that describe it when RAW does not follow (ADR 0023)", () => {
    const files = filesOf(photo(), false);
    expect(files.going).toEqual(["MGF_7273.JPG", "MGF_7273.JPG.xmp"]);
    expect(files.staying).toEqual(["MGF_7273.NEF", "MGF_7273.xmp"]);
  });

  it("moves a lone NEF's sidecars with it whatever the toggle", () => {
    const sidecars = [{ suffix: ".xmp", follows: "raw" }] as const;
    const lone = photo({ primary: ".NEF", raw: null, sidecars });
    expect(filesOf(lone, false)).toEqual({ going: ["MGF_7273.NEF", "MGF_7273.xmp"], staying: [] });
  });

  it("copies camera-protected photos, for keeps and rejects", () => {
    expect(keepVerb("move", photo())).toBe("moved");
    expect(keepVerb("copy", photo())).toBe("copied");
    expect(keepVerb("move", photo({ cameraProtected: true }))).toBe("copied");
    expect(rejectVerb(photo())).toBe("moved");
    expect(rejectVerb(photo({ cameraProtected: true }))).toBe("copied");
  });
});
