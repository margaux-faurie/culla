// What the install page can offer today. Edit this file when the first release is published
// and when each video is recorded (docs/videos.md); until then the page says so plainly and
// shows no download link, and no player for a video that is not recorded.

export interface Release {
  /** For example "1.0.0". */
  readonly version: string;
  /** The release page on GitHub, for example https://github.com/margaux-faurie/culla/releases/tag/v1.0.0 */
  readonly url: string;
}

export interface Captions {
  /** BCP 47 language tag, for example "en" or "fr". */
  readonly lang: string;
  readonly label: string;
  /** A WebVTT file served by the demo site itself, for example /videos/culla-demo.en.vtt */
  readonly src: string;
}

export interface Video {
  readonly title: string;
  /** An MP4 or WebM file served by the demo site itself (CSP: media from 'self' only). */
  readonly src: string;
  readonly type: "video/mp4" | "video/webm";
  /** A poster image served by the demo site itself. */
  readonly poster: string;
  readonly captions: readonly Captions[];
  /**
   * The video's on-screen text, line by line and in order, shown under the player: the video
   * has no sound, so this is its text alternative. Same lines as its English caption track.
   */
  readonly transcript: readonly string[];
}

export interface InstallStatus {
  readonly release: Release | null;
  readonly demoVideo: Video | null;
  readonly installVideo: Video | null;
}

export const STATUS: InstallStatus = {
  release: null,
  // Published by Margaux's decision of 28 Nov 2026 (the 30 s cut, v4 since the lean review; no sound yet). The video
  // and its poster hold her photographs: all rights reserved, not GPL (public-demo/videos/LICENSE.txt).
  demoVideo: {
    title: "Culla in 30 seconds",
    src: "/videos/culla-demo.mp4",
    type: "video/mp4",
    poster: "/videos/culla-demo-poster.jpg",
    captions: [
      { lang: "en", label: "English", src: "/videos/culla-demo.en.vtt" },
      { lang: "fr", label: "Français", src: "/videos/culla-demo.fr.vtt" },
    ],
    // Kept in step with public-demo/videos/culla-demo.en.vtt (src/install/markdown.test.ts).
    // "Nothing deleted. Ever." is on screen in this cut; its wording is open (SPEC §10).
    transcript: [
      "600 photos. One evening.",
      "Sorting them? Painful.",
      "Keep. Reject. Keep. Keep.",
      "The RAW follows.",
      "Nothing moves until you say so.",
      "Web demo: the apply is simulated.",
      "Nothing deleted. Ever.",
      "Culla. Free. Open source. Windows 11.",
      "github.com/margaux-faurie/culla",
    ],
  },
  installVideo: null,
};

/** Stands in for the page's own origin: only the origin comparison matters, not its value. */
const SELF = "https://self.invalid";

const RELEASES = "https://github.com/margaux-faurie/culla/releases/";

/**
 * True for a page under the project's GitHub Releases, the only place the page links a
 * download to.
 */
export function isReleaseUrl(url: string): boolean {
  if (!url.startsWith(RELEASES) || url.includes("\\")) {
    return false;
  }
  try {
    const parsed = new URL(url);
    return (
      parsed.origin === "https://github.com" &&
      parsed.pathname.startsWith("/margaux-faurie/culla/releases/")
    );
  } catch {
    return false;
  }
}

/**
 * A video source must be a path on the demo site itself: no third-party host, no embed. Browsers
 * read a backslash as a slash (`/\evil.com` is `//evil.com`), so any backslash is refused, and
 * the path must still resolve to the page's own origin.
 */
export function isSelfHosted(path: string): boolean {
  if (!path.startsWith("/") || path.includes("\\")) {
    return false;
  }
  try {
    return new URL(path, SELF).origin === SELF;
  } catch {
    return false;
  }
}
