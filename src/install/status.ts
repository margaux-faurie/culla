// What the install page can offer today. Edit this file when the first release is published
// and when each video is recorded (docs/videos.md); until then the page says so plainly and
// shows no download link or video player.

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
}

export interface InstallStatus {
  readonly release: Release | null;
  readonly demoVideo: Video | null;
  readonly installVideo: Video | null;
}

export const STATUS: InstallStatus = {
  release: null,
  demoVideo: null,
  installVideo: null,
};

/** A video source must be a path on the demo site itself: no third-party host, no embed. */
export function isSelfHosted(path: string): boolean {
  return path.startsWith("/") && !path.startsWith("//");
}
