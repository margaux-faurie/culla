import { useEffect, useState } from "react";
import { useBackend } from "../backend/context";
import type { PhotoExif, SessionPhoto } from "../backend/types";
import { exifRows } from "./exif-text";

/** How often the overlay asks again while the EXIF pass has not reached the photo. */
const PENDING_RETRY_MS = 400;

type Shown =
  | { readonly kind: "loading" }
  | { readonly kind: "failed" }
  | Extract<PhotoExif, { kind: "ready" }>;

/**
 * The EXIF overlay (US-40): the photo's date, camera, lens and exposure, as the backend writes
 * them. It follows the photo shown and stays open until I or Esc. It sits in the photo area, so it
 * never covers the Keep and Reject buttons or the header. Values are text, never markup.
 */
export function ExifOverlay({ photo }: { readonly photo: SessionPhoto }) {
  const backend = useBackend();
  const [shown, setShown] = useState<Shown>({ kind: "loading" });

  useEffect(() => {
    let active = true;
    let timer: number | undefined;
    setShown({ kind: "loading" });
    const ask = () => {
      backend.photoExif(photo.id).then(
        (exif) => {
          if (!active) return;
          if (exif.kind === "pending") {
            timer = window.setTimeout(ask, PENDING_RETRY_MS);
          } else {
            setShown(exif);
          }
        },
        () => {
          if (active) setShown({ kind: "failed" });
        },
      );
    };
    ask();
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [backend, photo.id]);

  return (
    <section className="exif" aria-labelledby="exif-title">
      <h2 id="exif-title" className="exif-title">
        Photo details · {photo.name}
      </h2>
      {shown.kind === "loading" && (
        <p className="small">
          Reading the photo details{" "}
          <span className="dots" aria-hidden="true">
            <span />
            <span />
            <span />
          </span>
        </p>
      )}
      {shown.kind === "failed" && <p className="small">Could not read the photo details.</p>}
      {shown.kind === "ready" && (
        <dl className="exif-list">
          {exifRows(shown).map((row) => (
            <div key={row.label} className="exif-row">
              <dt>{row.label}</dt>
              <dd>{row.value}</dd>
            </div>
          ))}
        </dl>
      )}
    </section>
  );
}
