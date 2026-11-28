import type { ReactNode } from "react";

/**
 * "Where photos go" in short (SPEC §2): one row per branch with a big count and a short label, the
 * same on the review and apply screens (decided by Margaux 28 Nov 2026; this layout proposed, SPEC
 * §10 "Lean review"). Every path in it is a display string from the backend, printed as it is.
 */
export function ShortRoute({ children }: { readonly children: ReactNode }) {
  return (
    <ul className="branches" aria-label="Where your photos will go">
      {children}
    </ul>
  );
}

export interface RouteBranchProps {
  readonly what: "kept" | "rejected" | "undecided";
  readonly count: number;
  readonly label: string;
  readonly children: ReactNode;
}

/** One branch in a sentence: "12 kept → …", "8 rejected → …", "4 left in place". */
export function RouteBranch({ what, count, label, children }: RouteBranchProps) {
  return (
    <li className={`branch branch-${what}`}>
      <h2 className="branch-head">
        <span className="branch-count">{count.toLocaleString("en-GB")}</span>{" "}
        <span className="branch-label">{label}</span>
      </h2>
      <div className="branch-body">{children}</div>
    </li>
  );
}

/** Where a branch goes: "→ Moved to E:\2026\_rejected", marked "new folder" while it does not exist. */
export function BranchTo({
  place,
  newFolder = false,
}: {
  readonly place: string;
  readonly newFolder?: boolean;
}) {
  return (
    <span className="branch-to">
      <span aria-hidden="true">→ </span>
      {place}
      {newFolder && (
        <>
          <span className="visually-hidden">, </span>
          <span className="pill pill-outline">new folder</span>
        </>
      )}
    </span>
  );
}
