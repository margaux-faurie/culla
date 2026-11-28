import { useRef } from "react";
import type { FullImage, SessionPhoto } from "../backend/types";
import type { Point, Rect, Size } from "./zoom";

/** Zoom on the current photo (US-39): waiting for the full image, or showing it at 100 %. */
export type Zoom =
  | {
      readonly phase: "loading";
      readonly photoId: string;
      /** Started by a held Space: Space up ends it. */
      readonly held: boolean;
      readonly view: Size;
      readonly fitted: Rect;
      readonly pointer: Point | null;
    }
  | {
      readonly phase: "shown";
      readonly photoId: string;
      readonly held: boolean;
      readonly view: Size;
      readonly image: FullImage;
      /** The full image's size in CSS pixels at 100 %. */
      readonly size: Size;
      /** Its top-left corner in the view. */
      readonly offset: Point;
    };

export interface ZoomLayerProps {
  readonly zoom: Zoom;
  readonly photo: SessionPhoto;
  /** A drag pans the photo while zoomed (proposed, US-39). */
  readonly onDrag: (dx: number, dy: number) => void;
}

/**
 * Over the stage while zoomed. While the full image loads, the preview stays visible under loading
 * dots; once loaded, the full image shows at 100 %, the preview scaled under it until it is drawn.
 * Nothing animates, so reduced motion gets the same instant swap.
 */
export function ZoomLayer({ zoom, photo, onDrag }: ZoomLayerProps) {
  const last = useRef<Point | null>(null);
  if (zoom.phase === "loading") {
    return (
      <div className="zoom-loading">
        <span className="pill pill-photo">
          Loading the full image
          <span className="dots" aria-hidden="true">
            <span />
            <span />
            <span />
          </span>
        </span>
      </div>
    );
  }
  const { size, offset, image } = zoom;
  const style = {
    width: `${size.w}px`,
    height: `${size.h}px`,
    transform: `translate(${offset.x}px, ${offset.y}px)`,
  };
  return (
    <div
      className="zoom"
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId);
        last.current = { x: event.clientX, y: event.clientY };
      }}
      onPointerMove={(event) => {
        const from = last.current;
        if (from === null) return;
        last.current = { x: event.clientX, y: event.clientY };
        onDrag(event.clientX - from.x, event.clientY - from.y);
      }}
      onPointerUp={() => {
        last.current = null;
      }}
      onPointerCancel={() => {
        last.current = null;
      }}
    >
      <img className="zoom-image" src={photo.previewUrl} alt="" style={style} draggable={false} />
      <img
        className="zoom-image"
        src={image.url}
        alt={`${photo.alt}, at 100 %`}
        style={style}
        draggable={false}
      />
      <p className="zoom-caption">
        <span className="pill pill-photo">{photo.name} · 100 %</span>
        {image.embeddedPreview && (
          <span className="pill pill-photo">Embedded preview, not the RAW</span>
        )}
      </p>
    </div>
  );
}
