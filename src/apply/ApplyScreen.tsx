import { useEffect, useMemo, useRef, useState } from "react";
import { useBackend } from "../backend/context";
import { isNotReady } from "../backend/errors";
import type {
  ApplyReport,
  ApplyStatus,
  Backend,
  CurrentFile,
  ReportGroup,
  SessionInfo,
} from "../backend/types";
import { holdText } from "../review/model";
import { RouteView } from "../route/RouteView";
import { LogDialog } from "./LogDialog";
import {
  type Announced,
  bytesLine,
  canContinue,
  doneLine,
  fileLine,
  fileText,
  haltText,
  interruptedText,
  missingText,
  outcomeNote,
  outcomeTitle,
  POLL_MS,
  pausedText,
  percent,
  photoLine,
  progressAnnouncement,
  ROLL_FORWARD_ON_ITS_OWN,
  reportActions,
  reportLead,
  reportSections,
  reportTitle,
  runningTitle,
  STOP_LABEL,
  stepLines,
  summaryParts,
  timeLeft,
} from "./model";

export interface ApplyScreenProps {
  readonly session: SessionInfo;
  readonly planId: string;
  /** Found interrupted at launch (US-38): the launch notice heads the screen. */
  readonly recovery?: boolean;
  /** The plan is closed: back to the start screen. */
  readonly onDone: () => void;
  /** The plan is closed: back to the review, which plans the rest again. Absent in recovery. */
  readonly onApplyRemaining?: () => void;
}

type Failure = "failed" | "notReady" | null;

/**
 * Apply screen (SPEC §4.2, US-36, US-38) on cream: per-file progress, pause and stop after the
 * current file, then the report, with the route view kept in sight. Everything it shows comes
 * from the backend's status as display strings; it sends back only the plan id and run ids.
 * Pause and stop are acknowledged at once and taken by the engine after the current file. Esc does
 * nothing here (proposed): nothing stops on a stray key.
 */
export function ApplyScreen(props: ApplyScreenProps) {
  const { session, planId, recovery = false, onApplyRemaining } = props;
  const backend = useBackend();
  const titleRef = useRef<HTMLHeadingElement>(null);
  const [status, setStatus] = useState<ApplyStatus | null>(null);
  const [failure, setFailure] = useState<Failure>(null);
  // The request the user just made, shown before the backend answers (NFR-PERF-13).
  const [asked, setAsked] = useState<"pause" | "cancel" | "continue" | null>(null);
  const [busy, setBusy] = useState(false);
  const [announcement, setAnnouncement] = useState("");
  const [log, setLog] = useState<string | null>(null);

  // Answers are applied in the order they were asked; a late poll never undoes a newer answer.
  const asking = useRef(0);
  const shown = useRef(0);
  const take = (n: number, next: ApplyStatus) => {
    if (n < shown.current) return;
    shown.current = n;
    setStatus(next);
  };
  const call = async (fn: (b: Backend) => Promise<ApplyStatus>): Promise<ApplyStatus | null> => {
    asking.current += 1;
    const n = asking.current;
    try {
      const next = await fn(backend);
      take(n, next);
      setFailure(null);
      return next;
    } catch (error) {
      setFailure(isNotReady(error) ? "notReady" : "failed");
      return null;
    }
  };
  const callRef = useRef(call);
  callRef.current = call;

  useEffect(() => {
    titleRef.current?.focus();
  }, []);

  // Start, or for an interrupted apply look first: it rolls forward on its own only if so decided.
  useEffect(() => {
    const begin = recovery && !ROLL_FORWARD_ON_ITS_OWN;
    callRef.current((b) => (begin ? b.applyStatus(planId) : b.startApply(planId)));
  }, [planId, recovery]);

  const kind = status?.state.kind ?? null;
  const live = kind !== "ended" && failure !== "notReady";
  useEffect(() => {
    if (!live) return;
    const timer = window.setInterval(() => {
      callRef.current((b) => b.applyStatus(planId));
    }, POLL_MS);
    return () => window.clearInterval(timer);
  }, [live, planId]);

  // The interrupted apply's volume is back: it rolls forward (US-38, proposed).
  const missing = status?.state.kind === "interrupted" ? status.state.missing : undefined;
  useEffect(() => {
    if (missing === null && ROLL_FORWARD_ON_ITS_OWN) {
      callRef.current((b) => b.startApply(planId));
    }
  }, [missing, planId]);

  // Closing the window while it runs asks first (US-36, proposed); the next launch finishes it.
  useEffect(() => {
    if (!live) return;
    const onBeforeUnload = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = "An apply is running. Close and finish it next time Culla opens?";
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  }, [live]);

  // The live region: requests and changes of state at once, progress at most every 10 photos or 5 s.
  const announceTimer = useRef<number | undefined>(undefined);
  const announce = (text: string) => {
    window.clearTimeout(announceTimer.current);
    setAnnouncement("");
    announceTimer.current = window.setTimeout(() => setAnnouncement(text), 50);
  };
  useEffect(() => () => window.clearTimeout(announceTimer.current), []);
  const announceRef = useRef(announce);
  announceRef.current = announce;
  const lastSaid = useRef<Announced>({ at: 0, done: 0 });
  const lastKind = useRef<string | null>(null);
  useEffect(() => {
    if (status === null) return;
    const state = status.state;
    const stateKey = state.kind === "paused" ? `paused-${state.reason}-${state.back}` : state.kind;
    if (stateKey !== lastKind.current) {
      const before = lastKind.current;
      lastKind.current = stateKey;
      if (state.kind === "paused") {
        announceRef.current(pausedText(state) ?? "Paused.");
      } else if (state.kind === "ended" && status.report !== null) {
        announceRef.current(`${reportTitle(status.report)}. ${reportLead(status.report)}`);
      } else if (state.kind === "running" && before !== null && before !== "running") {
        announceRef.current("Carrying on.");
      }
      return;
    }
    const now = Date.now();
    const said = progressAnnouncement(lastSaid.current, now, status.progress);
    if (said !== null && state.kind === "running") {
      lastSaid.current = { at: now, done: status.progress.photosDone };
      announceRef.current(said);
    }
  }, [status]);

  // A request is shown until the backend's state says it was taken.
  useEffect(() => {
    if (status === null) return;
    const s = status.state;
    if (asked === "continue" && s.kind !== "paused" && s.kind !== "interrupted") setAsked(null);
    if ((asked === "pause" || asked === "cancel") && (s.kind !== "running" || s.halt === asked)) {
      setAsked(null);
    }
  }, [status, asked]);

  // The report takes focus when it opens, and the progress when a run starts again from it.
  const reportOpen = status?.report !== null && status?.report !== undefined;
  const wasOpen = useRef(false);
  useEffect(() => {
    if (reportOpen !== wasOpen.current) {
      wasOpen.current = reportOpen;
      window.setTimeout(() => titleRef.current?.focus(), 0);
    }
  }, [reportOpen]);

  const pause = () => {
    if (status?.state.kind !== "running" || status.state.halt !== null || asked !== null) return;
    setAsked("pause");
    announce("Pausing after this file.");
    call((b) => b.pauseApply(planId));
  };
  const stop = () => {
    const s = status?.state;
    if (s === undefined || asked === "cancel") return;
    if (s.kind === "running" && s.halt === "cancel") return;
    if (s.kind !== "running" && s.kind !== "paused") return;
    setAsked("cancel");
    announce(
      s.kind === "paused" ? "Stopping." : (haltText({ kind: "running", halt: "cancel" }) ?? ""),
    );
    call((b) => b.stopApply(planId));
  };
  const carryOn = () => {
    const s = status?.state;
    if (s === undefined || asked !== null) return;
    if (s.kind === "paused" && canContinue(s)) {
      setAsked("continue");
      call((b) => b.continueApply(planId));
    } else if (s.kind === "interrupted" && s.missing === null) {
      setAsked("continue");
      call((b) => b.startApply(planId));
    }
  };

  const runAgain = async (fn: (b: Backend) => Promise<ApplyStatus>) => {
    if (busy) return;
    setBusy(true);
    await call(fn);
    setBusy(false);
  };
  const close = async (then: () => void) => {
    if (busy) return;
    setBusy(true);
    try {
      await backend.closeApply(planId);
      then();
    } catch (error) {
      setFailure(isNotReady(error) ? "notReady" : "failed");
      setBusy(false);
    }
  };
  const openLog = async (runId: string) => {
    try {
      const opened = await backend.openLog(runId);
      if (opened.kind === "text") {
        setLog(opened.text);
      } else {
        announce("The log opened in your text editor.");
      }
    } catch (error) {
      announce(
        isNotReady(error)
          ? "Not ready yet: the app cannot open logs in this version."
          : "Could not open the log.",
      );
    }
  };

  const example = useMemo(() => {
    const photo = session.photos.find((p) => p.id === session.exampleId);
    return photo === undefined ? null : { photo, route: photo.route };
  }, [session]);

  const report = status?.report ?? null;
  const title = heading(status, recovery);

  return (
    <>
      <main className="apply" aria-labelledby="apply-title">
        <header className="apply-header">
          <span className="brand">Culla</span>
          <span className="small">{session.source.folder.path}</span>
        </header>
        <div className="review-intro">
          <h1 id="apply-title" ref={titleRef} className="title" tabIndex={-1}>
            {title}
          </h1>
          {status !== null && report === null && status.state.kind !== "interrupted" && (
            <p className="lead">{doneLine(status.progress)}</p>
          )}
          {report !== null && <p className="lead">{reportLead(report)}</p>}
        </div>

        {recovery && report === null && (
          <p className="apply-notice">{interruptedText(ROLL_FORWARD_ON_ITS_OWN)}</p>
        )}
        {failure === "failed" && (
          <p className="save-error" role="alert">
            Culla lost track of the apply for a moment. It carries on: nothing is half-done.
          </p>
        )}
        {failure === "notReady" && (
          <p className="finding">Not ready yet: the app cannot apply a plan in this version.</p>
        )}

        {status !== null && report === null && (
          <Progress
            status={status}
            asked={asked}
            onPause={pause}
            onStop={stop}
            onContinue={carryOn}
          />
        )}

        {report !== null && (
          <Report
            report={report}
            busy={busy}
            onRetry={() => runAgain((b) => b.retryFailed(planId))}
            onFinish={() => runAgain((b) => b.finishMoves(planId))}
            {...(onApplyRemaining === undefined
              ? {}
              : { onApplyRemaining: () => close(onApplyRemaining) })}
            onOpenLog={() => openLog(report.runId)}
            onDone={() => close(props.onDone)}
          />
        )}

        <section className="panel apply-route" aria-labelledby="apply-route-title">
          <h2 id="apply-route-title" className="side-title">
            Where your photos go
          </h2>
          <RouteView
            source={session.source}
            photoSummary={`${status?.progress.photos ?? 0} photos in this apply`}
            example={example}
            rejectedFolder={session.rejectedFolder}
            rejectedFolderExists={session.rejectedFolderExists}
            destination={session.destination}
            keepMode={session.options.keepMode}
          />
        </section>

        <p className="visually-hidden" role="status">
          {announcement}
        </p>
      </main>
      <LogDialog text={log} onClose={() => setLog(null)} />
    </>
  );
}

function heading(status: ApplyStatus | null, recovery: boolean): string {
  if (status === null) return recovery ? "An apply was interrupted" : "Starting the apply";
  if (status.report !== null) return reportTitle(status.report);
  switch (status.state.kind) {
    case "interrupted":
      return "An apply was interrupted";
    case "paused":
      return "Paused";
    default:
      // The launch notice keeps its heading while it finishes the apply (US-38).
      return recovery ? "An apply was interrupted" : runningTitle(status.progress);
  }
}

interface ProgressProps {
  readonly status: ApplyStatus;
  readonly asked: "pause" | "cancel" | "continue" | null;
  readonly onPause: () => void;
  readonly onStop: () => void;
  readonly onContinue: () => void;
}

/** The bar, the current file and its steps, and the controls (canvas, Apply board). */
function Progress({ status, asked, onPause, onStop, onContinue }: ProgressProps) {
  const { state, progress } = status;
  const value = percent(progress);
  const halting =
    asked === "pause" || asked === "cancel"
      ? haltText({ kind: "running", halt: asked })
      : haltText(state);
  const pausing = asked === "pause" || (state.kind === "running" && state.halt === "pause");
  const stopping = asked === "cancel" || (state.kind === "running" && state.halt === "cancel");
  const paused = state.kind === "paused";
  const waiting = state.kind === "interrupted";

  return (
    <div className="apply-progress">
      {!waiting && (
        <div className="apply-meter">
          <div
            className="apply-bar"
            role="progressbar"
            aria-label="Apply progress"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={value}
            aria-valuetext={`${progress.photosDone} of ${progress.photos} photos, ${bytesLine(progress)}`}
          >
            <div className="apply-bar-fill" style={{ width: `${value}%` }} />
          </div>
          <div className="apply-meta">
            <span>{paused ? "Paused" : timeLeft(progress)}</span>
            <span>{bytesLine(progress)}</span>
          </div>
        </div>
      )}

      {state.kind === "interrupted" && state.missing !== null && (
        <p className="apply-alert" role="alert">
          {missingText(state.missing)}
        </p>
      )}
      {paused && <p className="apply-alert">{pausedText(state)}</p>}
      {status.current !== null && <Now current={status.current} />}
      {halting !== null && <p className="apply-halt">{halting}</p>}

      <div className="apply-controls">
        {waiting ? (
          !ROLL_FORWARD_ON_ITS_OWN && (
            <button
              type="button"
              className="button button-primary"
              aria-disabled={state.missing !== null}
              onClick={onContinue}
            >
              Continue the apply
            </button>
          )
        ) : (
          <>
            {/* One element for Pause and Continue, so focus stays on it when it changes. */}
            <button
              type="button"
              className={paused ? "button button-primary" : "button button-outline"}
              aria-disabled={paused ? !canContinue(state) || asked !== null : pausing || stopping}
              onClick={paused ? onContinue : onPause}
            >
              {paused ? "Continue" : pausing ? "Pausing" : "Pause"}
            </button>
            <button
              type="button"
              className="button button-outline"
              aria-disabled={stopping}
              onClick={onStop}
            >
              {stopping ? "Stopping" : paused ? "Stop here" : STOP_LABEL}
            </button>
          </>
        )}
        <span className="small">
          If the drive is unplugged, Culla finishes the job next time it is connected.
        </span>
      </div>
    </div>
  );
}

/** The current photo, its file and the file's steps. */
function Now({ current }: { readonly current: CurrentFile }) {
  return (
    <section className="apply-now" aria-labelledby="apply-now-title">
      <h2 id="apply-now-title" className="apply-now-label">
        Now
      </h2>
      <p className="apply-now-photo">{photoLine(current)}</p>
      {current.count > 1 && <p className="small">{fileLine(current)}</p>}
      <ol className="steps">
        {stepLines(current).map((step) => (
          <li key={step.text} className={`step step-${step.state}`}>
            <span className="step-mark" aria-hidden="true" />
            <span>
              {step.text}
              <span className="visually-hidden">
                {step.state === "done" ? ", done" : step.state === "current" ? ", now" : ", next"}
              </span>
            </span>
          </li>
        ))}
      </ol>
    </section>
  );
}

interface ReportProps {
  readonly report: ApplyReport;
  readonly busy: boolean;
  readonly onRetry: () => void;
  readonly onFinish: () => void;
  readonly onApplyRemaining?: () => void;
  readonly onOpenLog: () => void;
  readonly onDone: () => void;
}

/** The report (US-36, file-engine §9.6): counts, alarms first, each photo's files, the actions. */
function Report(props: ReportProps) {
  const { report } = props;
  const sections = reportSections(report);
  const actions = reportActions(report);
  const [doneOpen, setDoneOpen] = useState(false);

  return (
    <div className="report">
      <ul className="report-summary" aria-label="Summary">
        {summaryParts(report).map((part) => (
          <li key={part}>{part}</li>
        ))}
      </ul>

      {sections
        .filter((s) => s.outcome !== "done")
        .map((s) => (
          <section
            key={s.outcome}
            className={s.outcome === "alarm" ? "report-section report-alarm" : "report-section"}
            aria-labelledby={`report-${s.outcome}`}
          >
            <h2 id={`report-${s.outcome}`} className="finding-title">
              {outcomeTitle(s.outcome)}
            </h2>
            <p className="finding">{outcomeNote(s.outcome, report)}</p>
            <GroupList groups={s.groups} report={report} />
          </section>
        ))}

      {report.held.length > 0 && (
        <section className="report-section" aria-labelledby="report-held">
          <h2 id="report-held" className="finding-title">
            Held back
          </h2>
          <p className="finding">
            These photos were left out of the plan. They stay where they are.
          </p>
          <ul className="report-groups">
            {report.held.map((h) => (
              <li key={h.photoId}>
                <span className="strong">{h.name}</span> ({h.decision}, in {h.folder}):{" "}
                {holdText(h.reason)}.
              </li>
            ))}
          </ul>
        </section>
      )}

      {report.partialsKept.length > 0 && (
        <section className="report-section" aria-labelledby="report-partials">
          <h2 id="report-partials" className="finding-title">
            Partial copies kept
          </h2>
          <p className="finding">
            Their originals vanished, so these copies may be the only ones. Culla left them.
          </p>
          <ul className="report-groups">
            {report.partialsKept.map((path) => (
              <li key={path}>{path}</li>
            ))}
          </ul>
        </section>
      )}

      {sections
        .filter((s) => s.outcome === "done")
        .map((s) => (
          <section key="done" className="report-section" aria-labelledby="report-done">
            <h2 id="report-done" className="finding-title">
              {outcomeTitle("done")}
            </h2>
            <p className="finding">{outcomeNote("done", report)}</p>
            <div>
              <button
                type="button"
                className="button button-quiet button-small"
                aria-expanded={doneOpen}
                aria-controls="report-done-list"
                onClick={() => setDoneOpen((open) => !open)}
              >
                {doneOpen ? "Hide them" : `See them (${s.groups.length})`}
              </button>
            </div>
            {doneOpen && <GroupList id="report-done-list" groups={s.groups} report={report} />}
          </section>
        ))}

      <div className="report-actions">
        {actions.retry > 0 && (
          <button
            type="button"
            className="button button-outline"
            disabled={props.busy}
            onClick={props.onRetry}
          >
            Retry failed ({actions.retry})
          </button>
        )}
        {actions.finish > 0 && (
          <button
            type="button"
            className="button button-outline"
            disabled={props.busy}
            onClick={props.onFinish}
          >
            Finish moves ({actions.finish})
          </button>
        )}
        {actions.remaining > 0 && props.onApplyRemaining !== undefined && (
          <button
            type="button"
            className="button button-outline"
            disabled={props.busy}
            onClick={props.onApplyRemaining}
          >
            Apply remaining ({actions.remaining})
          </button>
        )}
        <button type="button" className="button button-outline" onClick={props.onOpenLog}>
          Open log
        </button>
        <button
          type="button"
          className="button button-primary"
          disabled={props.busy}
          onClick={props.onDone}
        >
          Done
        </button>
      </div>
      <p className="small">Each action runs only when you choose it. Done closes this apply.</p>
    </div>
  );
}

function GroupList({
  groups,
  report,
  id,
}: {
  readonly groups: readonly ReportGroup[];
  readonly report: ApplyReport;
  readonly id?: string;
}) {
  return (
    <ul id={id} className="report-groups">
      {groups.map((g) => (
        <li key={g.photoId}>
          <span className="strong">{g.name}</span>{" "}
          <span className="muted">· {g.action === "keep" ? "kept" : "rejected"}</span>
          <ul className="report-files">
            {g.files.map((f) => (
              <li key={f.fileName}>{fileText(f, g, report.end)}</li>
            ))}
          </ul>
        </li>
      ))}
    </ul>
  );
}
