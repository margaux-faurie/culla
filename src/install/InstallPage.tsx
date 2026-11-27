import installGuide from "../../docs/install.md?raw";
import { guideForWeb } from "./guide";
import { MarkdownBlocks } from "./MarkdownView";
import { isSelfHosted, STATUS, type Video } from "./status";

const guide = guideForWeb(installGuide);

/**
 * The download and install page of the demo site (SPEC §2, decided 28 Nov 2026): the content of
 * docs/install.md, the state of the first release and a place for the two videos.
 */
export function InstallPage() {
  const { release, demoVideo, installVideo } = STATUS;
  return (
    <div className="doc-page">
      <header className="start-header doc-header">
        <a className="brand" href="/">
          Culla
        </a>
        <a href="/">Try the demo</a>
      </header>
      <main className="doc" aria-labelledby={guide.title?.id}>
        {guide.title !== null && <MarkdownBlocks blocks={[guide.title]} />}

        <section className="doc-status" aria-label="Download">
          {release === null ? (
            <p className="doc-status-line">
              <strong>There is no release yet.</strong> Culla is still being built, so there is
              nothing to download. The guide below shows how installing will work once the first
              version is out. Until then, <a href="/">try the demo</a>.
            </p>
          ) : (
            <p className="doc-status-line">
              <strong>Culla {release.version} is out.</strong>{" "}
              <a href={release.url}>Download it from GitHub Releases</a>, then follow the steps
              below.
            </p>
          )}
        </section>

        <section className="doc-videos" aria-labelledby="videos">
          <h2 id="videos">Videos</h2>
          {demoVideo === null && installVideo === null ? (
            <p>
              The demo video and the install video are not recorded yet. They will appear here, with
              captions in English and French.
            </p>
          ) : (
            <>
              <VideoBlock video={demoVideo} missing="The demo video is not recorded yet." />
              <VideoBlock video={installVideo} missing="The install video is not recorded yet." />
            </>
          )}
        </section>

        <MarkdownBlocks blocks={guide.body} />
      </main>
    </div>
  );
}

function VideoBlock({
  video,
  missing,
}: {
  readonly video: Video | null;
  readonly missing: string;
}) {
  if (video === null || !isSelfHosted(video.src) || !isSelfHosted(video.poster)) {
    return <p>{missing}</p>;
  }
  // Captions are required (docs/videos.md): without a self-hosted caption track, no player.
  const captions = video.captions.filter((track) => isSelfHosted(track.src));
  const [first, ...others] = captions;
  if (first === undefined) {
    return <p>{missing}</p>;
  }
  return (
    <figure className="doc-video">
      {/* No autoplay; captions as WebVTT tracks, the first shown by default. */}
      <video controls preload="none" poster={video.poster} width={1920} height={1080}>
        <source src={video.src} type={video.type} />
        <track kind="captions" srcLang={first.lang} label={first.label} src={first.src} default />
        {others.map((track) => (
          <track
            key={track.lang}
            kind="captions"
            srcLang={track.lang}
            label={track.label}
            src={track.src}
          />
        ))}
        <a href={video.src}>Download the video</a>
      </video>
      <figcaption>{video.title}</figcaption>
    </figure>
  );
}
