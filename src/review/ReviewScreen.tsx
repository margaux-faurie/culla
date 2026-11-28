import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useBackend } from "../backend/context";
import { isNoKeepRoute, isNotReady } from "../backend/errors";
import type {
  Blocker,
  KeepMode,
  ReviewPlan,
  SessionChange,
  SessionInfo,
  SessionPhoto,
  VolumeChoice,
} from "../backend/types";
import { leftInPlaceNote } from "../route/places";
import { BranchTo, RouteBranch, ShortRoute } from "../route/ShortRoute";
import { counts, type SwipeState, stateAt } from "../swipe/session";
import { ConfirmDialog } from "./ConfirmDialog";
import { KeepControls } from "./KeepControls";
import {
  blockerText,
  branchFiles,
  branchTitle,
  bytesText,
  canApply,
  disabledReason,
  estimateText,
  findingsCount,
  folderNote,
  heldLine,
  heldSummary,
  keepRouteHint,
  kindText,
  otherFilesText,
  plannedPhotos,
  primaryLabel,
  rawNote,
  readySummary,
  replannedSummary,
  replannedText,
  visibleNotes,
  worthKnowing,
} from "./model";

export interface ReviewScreenProps {
  readonly session: SessionInfo;
  readonly state: SwipeState;
  /** Every decision in `state.log` has reached the backend: the plan can be built from them. */
  readonly saved: boolean;
  readonly saveError: boolean;
  /** The backend answered a change of options (move or copy, destination, RAW follows). */
  readonly onSessionChanged: (session: SessionInfo) => void;
  readonly onBack: () => void;
  /** Back to sorting on one photo (a grid thumbnail). */
  readonly onShow: (photoId: string) => void;
  /** Back to sorting on the first undecided photo. */
  readonly onSortUndecided: () => void;
  /** The plan is confirmed: the apply screen takes over. */
  readonly onConfirmed: (planId: string) => void;
}

/** `needsRoute`: kept photos need move or copy and a connected destination before a plan. */
type Status = "checking" | "ready" | "failed" | "notReady" | "needsRoute";

/** How many thumbnails a strip shows before "See all". */
const STRIP_PREVIEW = 12;

/**
 * Review screen (SPEC §4.2, US-34, US-35) on cream. Lean review decided by Margaux (28 Nov 2026:
 * essentials visible, details folded); this layout is proposed, SPEC §10 "Lean review": the short
 * route with move or copy and the destination in the Kept branch and one thumbnail strip per
 * branch; beside it what stops the apply, the camera-protected and cloud-synced notes, and Apply
 * behind a confirm dialog; the findings, the time and size, every file and the options folded
 * under the route. The plan comes from the backend as display strings; nothing here builds a path. Opening it changes no
 * file: the backend's plan and preflight are read-only (INV-03). While kept photos have no route yet
 * (move or copy, a connected destination), no plan is built and Apply says what to choose.
 *
 * Only Esc is a shortcut here, so the swipe screen's single keys do nothing (NFR-A11Y-10).
 */
export function ReviewScreen(props: ReviewScreenProps) {
  const { session, state, saved, onBack } = props;
  const backend = useBackend();
  const titleRef = useRef<HTMLHeadingElement>(null);
  const applyRef = useRef<HTMLButtonElement>(null);
  const findingsRef = useRef<HTMLHeadingElement>(null);
  const [plan, setPlan] = useState<ReviewPlan | null>(null);
  const [status, setStatus] = useState<Status>("checking");
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [announcement, setAnnouncement] = useState("");
  const [changeError, setChangeError] = useState<string | null>(null);
  const [changing, setChanging] = useState(false);
  // The choice being sent, shown at once so the radio or checkbox does not flick back meanwhile.
  const [pending, setPending] = useState<SessionChange | null>(null);
  // Counts the backend's answers to `updateSession`: each one drops the draft, so each plans again.
  const [revision, setRevision] = useState(0);
  const uptoSeq = state.log.at(-1)?.seq ?? 0;
  const tally = counts(state);
  const { keepMode, rawFollows } = session.options;
  const byId = useMemo(() => new Map(session.photos.map((p) => [p.id, p])), [session.photos]);

  // Focus goes to the heading, never to Apply: Enter pressed again or held on the swipe screen
  // never reaches the primary button (US-35).
  useEffect(() => {
    titleRef.current?.focus();
  }, []);

  // A draft plan is built and preflighted each time the review opens, once the decisions are
  // saved; going back, changing decisions and coming back builds a new one (§7.4). Only the answer
  // to the latest request is shown.
  const generation = useRef(0);
  /**
   * Puts a message in the status region. It is cleared first, so the same words said twice (two
   * "Check again" with the same outcome) are read out again.
   */
  const announceTimer = useRef<number | undefined>(undefined);
  const announce = (text: string) => {
    window.clearTimeout(announceTimer.current);
    setAnnouncement("");
    announceTimer.current = window.setTimeout(() => setAnnouncement(text), 50);
  };
  useEffect(() => () => window.clearTimeout(announceTimer.current), []);
  // `announced`: the user asked for this check ("Check again"), so its outcome is read out.
  const check = (seq: number, announced = false) => {
    generation.current += 1;
    const mine = generation.current;
    setStatus("checking");
    if (announced) {
      window.clearTimeout(announceTimer.current);
      setAnnouncement("");
    }
    backend.reviewPlan(session.id, seq).then(
      (next) => {
        if (generation.current === mine) {
          setPlan(next);
          setStatus("ready");
          if (announced) {
            announce(outcomeText(next));
          }
        }
      },
      (error: unknown) => {
        if (generation.current === mine) {
          if (isNoKeepRoute(error)) {
            setStatus("needsRoute");
            return;
          }
          setStatus(isNotReady(error) ? "notReady" : "failed");
          if (announced) {
            announce(FAILED_TEXT);
          }
        }
      },
    );
  };
  const checkRef = useRef(check);
  checkRef.current = check;
  // Every answer to a change of options builds a new draft too, even when nothing changed: the
  // backend dropped the old one, which can no longer be confirmed (§7.4).
  useEffect(() => {
    if (saved) {
      checkRef.current(uptoSeq);
    }
    void revision;
  }, [saved, uptoSeq, revision]);
  useEffect(
    () => () => {
      generation.current += 1;
    },
    [],
  );

  // Esc goes back to sorting, unless the confirm dialog is open (it closes itself on Esc).
  const backRef = useRef(onBack);
  backRef.current = onBack;
  const dialogOpen = useRef(false);
  dialogOpen.current = confirmOpen;
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.code !== "Escape" || dialogOpen.current || event.defaultPrevented) {
        return;
      }
      if (event.target instanceof Element && event.target.closest("dialog") !== null) {
        return;
      }
      event.preventDefault();
      backRef.current();
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, []);

  const ready = status === "ready" && plan !== null;
  const applyEnabled = ready && canApply(plan) && !confirming && !changing;
  const reason =
    status === "needsRoute"
      ? keepRouteHint(keepMode, session.destination)
      : ready
        ? disabledReason(plan)
        : null;

  // One change at a time; the controls stay focusable meanwhile (a disabled control loses focus).
  const changingRef = useRef(false);
  const change = async (next: SessionChange) => {
    if (changingRef.current) {
      return;
    }
    changingRef.current = true;
    setChanging(true);
    setPending(next);
    setChangeError(null);
    try {
      props.onSessionChanged(await backend.updateSession(session.id, next));
      // The draft on screen is gone in the backend: never show it as confirmable meanwhile.
      generation.current += 1;
      setStatus("checking");
      setRevision((n) => n + 1);
    } catch (error) {
      setChangeError(
        isNotReady(error)
          ? "Not ready yet: the app cannot change this in this version."
          : "Could not change this. Nothing has moved.",
      );
    } finally {
      changingRef.current = false;
      setChanging(false);
      setPending(null);
    }
  };
  const chooseKeepMode = (mode: KeepMode) => change({ keepMode: mode });
  const chooseDestination = async () => {
    setChangeError(null);
    try {
      const picked = await backend.chooseDestination();
      if (picked !== null) {
        await change({ destinationId: picked.id });
      }
    } catch (error) {
      setChangeError(
        isNotReady(error)
          ? "Not ready yet: the app cannot open the folder dialog in this version."
          : "Could not open the folder dialog.",
      );
    }
  };
  const decidedIds = (wanted: "kept" | "rejected") =>
    state.photos.filter((_, i) => stateAt(state, i) === wanted);
  const heldIds = new Set(ready ? plan.held.map((h) => h.photoId) : []);

  const closeDialog = (focusApply: boolean) => {
    setConfirmOpen(false);
    window.setTimeout(() => {
      if (focusApply && applyRef.current !== null && !applyRef.current.disabled) {
        applyRef.current.focus();
      } else {
        findingsRef.current?.focus();
      }
    }, 0);
  };

  // A second activation while the first confirm is in flight does nothing: the state flag only
  // disables the button at the next render, the ref holds at once.
  const confirmingRef = useRef(false);
  const confirm = async () => {
    if (plan === null || confirmingRef.current) {
      return;
    }
    confirmingRef.current = true;
    generation.current += 1;
    setConfirming(true);
    try {
      const result = await backend.confirmPlan(plan.planId);
      if (result.kind === "confirmed") {
        props.onConfirmed(result.planId);
        return;
      }
      setPlan(result.plan);
      setStatus("ready");
      announce(
        result.kind === "blocked"
          ? "Nothing has started. A new problem stops the apply: see Needs fixing first."
          : "Nothing has started. The plan changed since you checked it: look at it again, then apply.",
      );
      closeDialog(false);
    } catch (error) {
      setStatus(isNotReady(error) ? "notReady" : "failed");
      closeDialog(false);
    } finally {
      confirmingRef.current = false;
      setConfirming(false);
    }
  };

  const chooseVolume = async (choice: VolumeChoice) => {
    if (plan === null) {
      return;
    }
    generation.current += 1;
    const mine = generation.current;
    setStatus("checking");
    window.clearTimeout(announceTimer.current);
    setAnnouncement("");
    try {
      const next = await backend.chooseVolume(plan.planId, choice.id);
      if (generation.current !== mine) {
        return;
      }
      setPlan(next);
      setStatus("ready");
      announce(`Using ${choice.label}. ${outcomeText(next)}`);
    } catch (error) {
      if (generation.current !== mine) {
        return;
      }
      setStatus(isNotReady(error) ? "notReady" : "failed");
      announce(FAILED_TEXT);
    }
  };

  // The folds below the route; "Details" beside a visible line opens the findings and moves there.
  const [folds, setFolds] = useState<ReadonlySet<Fold>>(new Set());
  const toggleFold = (fold: Fold) =>
    setFolds((open) => {
      const next = new Set(open);
      if (!next.delete(fold)) {
        next.add(fold);
      }
      return next;
    });
  const showFindings = () => {
    setFolds((open) => new Set(open).add("found"));
    window.setTimeout(() => document.getElementById("found-toggle")?.focus(), 0);
  };

  const found = ready ? findingsCount(plan) : 0;
  const summary = ready ? readySummary(plan) : null;
  // Notes worth knowing, beyond the held and re-planned photos that have their own line.
  const notes = ready ? found - plan.held.length - plan.replanned.length : 0;

  return (
    <>
      <main className="review" aria-labelledby="review-title">
        <header className="review-header">
          <span className="brand">Culla</span>
          <button type="button" className="button button-outline" onClick={onBack}>
            Back to sorting
          </button>
        </header>
        <div className="review-intro">
          <h1 id="review-title" ref={titleRef} className="title" tabIndex={-1}>
            Check where your photos will go
          </h1>
          <p className="lead">
            From {session.source.folder.path}, {session.source.folder.volume}. Nothing has moved
            yet.
          </p>
        </div>
        {props.saveError && (
          <p className="save-error" role="alert">
            Could not save the last decisions. The plan waits for them: go back to sorting and they
            are saved again with the next change.
          </p>
        )}

        <div className="review-main">
          <div className="review-plan">
            <ShortRoute>
              {tally.kept > 0 && (
                <RouteBranch what="kept" count={tally.kept} label="kept">
                  <KeepControls
                    keepMode={pending?.keepMode ?? keepMode}
                    destination={session.destination}
                    sameAsLastTime={session.sameAsLastTime}
                    onKeepMode={chooseKeepMode}
                    onChooseDestination={chooseDestination}
                  />
                  <span className="branch-note">{rawNote(pending?.rawFollows ?? rawFollows)}</span>
                  <PhotoStrip
                    what="kept"
                    photoIds={decidedIds("kept")}
                    heldIds={heldIds}
                    byId={byId}
                    onShow={props.onShow}
                  />
                </RouteBranch>
              )}
              {tally.rejected > 0 && (
                <RouteBranch what="rejected" count={tally.rejected} label="rejected">
                  <BranchTo
                    place={`Moved to ${session.rejectedFolder}`}
                    newFolder={!session.rejectedFolderExists}
                  />
                  <span className="branch-note">Nothing is deleted.</span>
                  <PhotoStrip
                    what="rejected"
                    photoIds={decidedIds("rejected")}
                    heldIds={heldIds}
                    byId={byId}
                    onShow={props.onShow}
                  />
                </RouteBranch>
              )}
              {tally.undecided > 0 && (
                <RouteBranch what="undecided" count={tally.undecided} label="left in place">
                  <span className="branch-note">{leftInPlaceNote(0)}</span>
                  <span>
                    <button
                      type="button"
                      className="button button-quiet button-small"
                      onClick={props.onSortUndecided}
                    >
                      Sort them now
                    </button>
                  </span>
                </RouteBranch>
              )}
            </ShortRoute>
            {changeError !== null && (
              <p className="error" role="alert">
                {changeError}
              </p>
            )}
          </div>

          <aside
            className="review-side"
            aria-labelledby="findings-title"
            aria-busy={status === "checking"}
          >
            <h2 id="findings-title" ref={findingsRef} className="side-title" tabIndex={-1}>
              Before you apply
            </h2>
            {status === "checking" && (
              <p className="checking">
                Checking
                <Dots />
              </p>
            )}
            {status === "failed" && (
              <div className="side-line">
                <p className="finding">Could not check the plan. Nothing has moved.</p>
                <CheckAgain onClick={() => check(uptoSeq, true)} />
              </div>
            )}
            {status === "notReady" && (
              <p className="finding">Not ready yet: the app cannot build a plan in this version.</p>
            )}
            {ready && plan.blockers.length > 0 && (
              <Blockers
                plan={plan}
                onCheckAgain={() => check(uptoSeq, true)}
                onChooseVolume={chooseVolume}
              />
            )}
            {ready && plan.replanned.length > 0 && (
              <SideLine text={replannedSummary(plan.replanned.length)} onDetails={showFindings} />
            )}
            {ready && plan.held.length > 0 && (
              <SideLine
                text={heldSummary(plan.held.length, plannedPhotos(plan))}
                onDetails={showFindings}
              />
            )}
            {ready &&
              visibleNotes(plan).map((text) => (
                <SideLine key={text} text={text} onDetails={null} />
              ))}
            {summary !== null && (
              <SideLine text={summary} onDetails={notes > 0 ? showFindings : null} />
            )}

            <div className="apply-box">
              <button
                ref={applyRef}
                type="button"
                className="button button-primary button-large"
                disabled={!applyEnabled}
                aria-describedby={reason === null ? undefined : "apply-reason"}
                onClick={() => setConfirmOpen(true)}
              >
                {ready ? primaryLabel(plan) : "Apply"}
              </button>
              {reason !== null && (
                <p id="apply-reason" className="small apply-reason">
                  {reason}
                </p>
              )}
              <p className="small apply-note">
                If anything stops it, Culla finishes the job next time.
              </p>
            </div>
          </aside>

          <div className="folds">
            {ready && found > 0 && (
              <FoldOut
                id="found"
                label={`What the check found (${found})`}
                open={folds.has("found")}
                onToggle={() => toggleFold("found")}
              >
                <Findings plan={plan} />
              </FoldOut>
            )}
            {ready && (
              <FoldOut
                id="estimate"
                label="Time and size"
                open={folds.has("estimate")}
                onToggle={() => toggleFold("estimate")}
              >
                <Estimate plan={plan} />
              </FoldOut>
            )}
            {ready && (
              <FoldOut
                id="files"
                label={`See every file (${plan.operations.length})`}
                open={folds.has("files")}
                onToggle={() => toggleFold("files")}
              >
                <FileList plan={plan} />
              </FoldOut>
            )}
            <FoldOut
              id="options"
              label="Options"
              open={folds.has("options")}
              onToggle={() => toggleFold("options")}
            >
              <label className="check">
                <input
                  type="checkbox"
                  checked={pending?.rawFollows ?? rawFollows}
                  onChange={(e) => change({ rawFollows: e.currentTarget.checked })}
                />
                RAW and edit files follow their JPEG
              </label>
              <span className="small">Photos in subfolders are always included.</span>
            </FoldOut>
          </div>
        </div>

        <p className="visually-hidden" role="status">
          {announcement}
        </p>
      </main>
      {ready && (
        <ConfirmDialog
          open={confirmOpen}
          plan={plan}
          busy={confirming}
          onCancel={() => closeDialog(true)}
          onConfirm={confirm}
          onClosed={() => {
            // Only a close the state did not ask for: a forced close leaves Apply able to reopen it.
            if (dialogOpen.current) {
              closeDialog(true);
            }
          }}
        />
      )}
    </>
  );
}

/** The folds under the route (lean review decided by Margaux; this layout proposed, SPEC §10). */
type Fold = "found" | "estimate" | "files" | "options";

function Dots() {
  return (
    <span className="dots" aria-hidden="true">
      <span />
      <span />
      <span />
    </span>
  );
}

function CheckAgain({ onClick }: { readonly onClick: () => void }) {
  return (
    <button type="button" className="button button-quiet button-small" onClick={onClick}>
      Check again
    </button>
  );
}

/** A short visible line in the side, with "Details" when the folded findings say more. */
function SideLine({
  text,
  onDetails,
}: {
  readonly text: string;
  readonly onDetails: (() => void) | null;
}) {
  return (
    <p className="side-line">
      <span>{text}</span>
      {onDetails !== null && (
        <>
          {" "}
          <button type="button" className="link-button" onClick={onDetails}>
            Details
          </button>
        </>
      )}
    </p>
  );
}

interface FoldOutProps {
  readonly id: Fold;
  readonly label: string;
  readonly open: boolean;
  readonly onToggle: () => void;
  readonly children: ReactNode;
}

/** A disclosure: a button that shows or hides what it controls. */
function FoldOut({ id, label, open, onToggle, children }: FoldOutProps) {
  return (
    <div className={open ? "fold fold-open" : "fold"}>
      <button
        id={`${id}-toggle`}
        type="button"
        className="fold-toggle"
        aria-expanded={open}
        aria-controls={`${id}-body`}
        onClick={onToggle}
      >
        <span className="fold-mark" aria-hidden="true" />
        {label}
      </button>
      {open && (
        <div id={`${id}-body`} className="fold-body">
          {children}
        </div>
      )}
    </div>
  );
}

interface PhotoStripProps {
  readonly what: "kept" | "rejected";
  readonly photoIds: readonly string[];
  readonly heldIds: ReadonlySet<string>;
  readonly byId: ReadonlyMap<string, SessionPhoto>;
  readonly onShow: (photoId: string) => void;
}

/** One strip of thumbnails of the decided photos; each one goes back to sorting on that photo. */
function PhotoStrip({ what, photoIds, heldIds, byId, onShow }: PhotoStripProps) {
  const [all, setAll] = useState(false);
  const shown = all ? photoIds : photoIds.slice(0, STRIP_PREVIEW);
  const label = what === "kept" ? "Kept photos" : "Rejected photos";
  return (
    <div className="strip">
      <ul className="grid" aria-label={`${label}, ${photoIds.length}`}>
        {shown.map((id) => {
          const photo = byId.get(id);
          if (photo === undefined) {
            return null;
          }
          const held = heldIds.has(id);
          return (
            <li key={id}>
              <button
                type="button"
                className={what === "rejected" ? "thumb thumb-rejected" : "thumb"}
                aria-label={`${photo.name}${held ? ", held back" : ""}. Show it in sorting`}
                onClick={() => onShow(id)}
              >
                <img src={photo.previewUrl} alt="" loading="lazy" decoding="async" />
                {held && (
                  <span className="thumb-note" aria-hidden="true">
                    Held
                  </span>
                )}
              </button>
            </li>
          );
        })}
      </ul>
      {photoIds.length > STRIP_PREVIEW && (
        <button
          type="button"
          className="link-button strip-more"
          aria-expanded={all}
          onClick={() => setAll((open) => !open)}
        >
          {all ? "Show fewer" : `See all ${photoIds.length}`}
        </button>
      )}
    </div>
  );
}

interface BlockersProps {
  readonly plan: ReviewPlan;
  readonly onCheckAgain: () => void;
  readonly onChooseVolume: (choice: VolumeChoice) => void;
}

/** What stops the apply stays in view, each in one sentence with what to do (FE §8). */
function Blockers({ plan, onCheckAgain, onChooseVolume }: BlockersProps) {
  return (
    <section className="finding-blockers" aria-labelledby="blockers-title">
      <h3 id="blockers-title" className="finding-title">
        Needs fixing first
      </h3>
      <ul className="findings">
        {plan.blockers.map((b) => (
          <li key={blockerKey(b)} className="finding finding-blocker">
            <span className="finding-mark finding-mark-blocker" aria-hidden="true" />
            <span>
              {blockerText(b)}
              {b.kind === "volumeAmbiguous" && (
                <span className="volume-choices">
                  {b.choices.map((c) => (
                    <button
                      key={c.id}
                      type="button"
                      className="button button-quiet button-small"
                      onClick={() => onChooseVolume(c)}
                    >
                      Use {c.label}
                    </button>
                  ))}
                </span>
              )}
            </span>
          </li>
        ))}
      </ul>
      <CheckAgain onClick={onCheckAgain} />
    </section>
  );
}

/** The folded findings: re-plans, held photos, then what is worth knowing (FE §8). */
function Findings({ plan }: { readonly plan: ReviewPlan }) {
  const [othersOpen, setOthersOpen] = useState(false);
  const notes = worthKnowing(plan);
  const others = otherFilesText(plan.otherFiles);
  return (
    <>
      {plan.replanned.length > 0 && (
        <FindingGroup title="Changed since you checked" lines={plan.replanned.map(replannedText)} />
      )}
      {plan.held.length > 0 && <FindingGroup title="Held back" lines={plan.held.map(heldLine)} />}
      {(notes.length > 0 || others !== null) && (
        <section className="finding-group" aria-labelledby="worth-title">
          <h3 id="worth-title" className="finding-title">
            Worth knowing
          </h3>
          <ul className="findings">
            {notes.map((text) => (
              <li key={text} className="finding">
                <span className="finding-mark" aria-hidden="true" />
                <span>{text}</span>
              </li>
            ))}
            {others !== null && (
              <li className="finding">
                <span className="finding-mark" aria-hidden="true" />
                <span>
                  {others}{" "}
                  <button
                    type="button"
                    className="link-button"
                    aria-expanded={othersOpen}
                    aria-controls="other-files"
                    onClick={() => setOthersOpen((open) => !open)}
                  >
                    {othersOpen ? "Hide them" : "Show them"}
                  </button>
                  {othersOpen && (
                    <ul id="other-files" className="other-files">
                      {plan.otherFiles.map((f) => (
                        <li key={f.path}>{f.path}</li>
                      ))}
                    </ul>
                  )}
                </span>
              </li>
            )}
          </ul>
        </section>
      )}
    </>
  );
}

function FindingGroup({ title, lines }: { readonly title: string; readonly lines: string[] }) {
  return (
    <section className="finding-group">
      <h3 className="finding-title">{title}</h3>
      <ul className="findings">
        {lines.map((text) => (
          <li key={text} className="finding">
            <span className="finding-mark" aria-hidden="true" />
            <span>{text}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** The time and size of the apply (US-34), and the files of each branch. */
function Estimate({ plan }: { readonly plan: ReviewPlan }) {
  const bytes = bytesText(plan);
  return (
    <>
      <p className="finding-text">
        {estimateText(plan)}
        {bytes !== null && <> {bytes}.</>}
        {!plan.estimate.measured && plan.estimate.bytesToWrite > 0 && (
          <> A first estimate, until Culla has timed this drive.</>
        )}
      </p>
      {plan.kept !== null && <p className="small">Kept: {branchFiles(plan.kept)}</p>}
      {plan.rejected !== null && <p className="small">Rejected: {branchFiles(plan.rejected)}</p>}
    </>
  );
}

const blockerKey = (b: Blocker): string => {
  const role = "role" in b ? `-${b.role}` : "";
  const volume = "volume" in b ? `-${b.volume}` : "";
  return `${b.kind}${role}${volume}`;
};

const FAILED_TEXT = "Could not check the plan. Nothing has moved.";

/** What a check the user asked for found, for the status region. */
const outcomeText = (plan: ReviewPlan): string =>
  plan.blockers.length > 0
    ? `Checked again. ${plan.blockers.length === 1 ? "1 problem still needs" : `${plan.blockers.length} problems still need`} fixing first.`
    : canApply(plan)
      ? "Checked again. Nothing stops the apply."
      : "Checked again. Nothing to apply yet.";

/**
 * Every file of the plan: the target folders of each branch, then every operation with its source,
 * target (suffix included) and kind, and the files that stay.
 */
function FileList({ plan }: { readonly plan: ReviewPlan }) {
  const branches = [
    [plan.kept, "kept"],
    [plan.rejected, "rejected"],
  ] as const;
  const label = `Every file (${plan.operations.length})`;
  return (
    <>
      {branches.map(([branch, what]) =>
        branch === null ? null : (
          <div key={what} className="file-folders">
            <span className="finding-title">{branchTitle(branch, what)}</span>
            <ul className="folder-list" aria-label={`Folders for ${what} photos`}>
              {branch.folders.map((f) => (
                <li key={f.folder}>
                  {f.folder} <span className="muted">· {folderNote(f.photos, f.newFolder)}</span>
                </li>
              ))}
            </ul>
          </div>
        ),
      )}
      {/* biome-ignore lint/a11y/noNoninteractiveTabindex: a named region that scrolls sideways at 320 px takes focus so the keyboard can scroll it (WCAG 2.1.1, axe scrollable-region-focusable). */}
      <section className="table-wrap" tabIndex={0} aria-label={label}>
        <table className="file-table">
          <thead>
            <tr>
              <th scope="col">Photo</th>
              <th scope="col">From</th>
              <th scope="col">To</th>
              <th scope="col">How</th>
            </tr>
          </thead>
          <tbody>
            {plan.operations.map((op) => (
              <tr key={op.id}>
                <td>
                  {op.photoName}{" "}
                  <span className="muted">· {op.action === "keep" ? "kept" : "rejected"}</span>
                </td>
                <td>{op.from}</td>
                <td>{op.to}</td>
                <td>{kindText(op.kind, op.cameraProtected)}</td>
              </tr>
            ))}
            {plan.staying.map((file) => (
              <tr key={file.path}>
                <td>{file.photoName}</td>
                <td>{file.path}</td>
                <td>Stays where it is</td>
                <td>Stays: "RAW and edit files follow their JPEG" is off</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
    </>
  );
}
