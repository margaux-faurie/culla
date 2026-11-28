# Deploying the web demo

Status: 27 Nov 2026. The web demo is the `src/` build in demo mode (ADR 0014), served as static assets by a Cloudflare Worker named `culla` at `culla.margauxfaurie.com`, behind Cloudflare Access until the portfolio opens (SPEC §2). Workers Builds builds and deploys it from this repository.

Margaux decided (27 Nov 2026): the Worker `culla` deployed by Workers Builds, the custom domain `culla.margauxfaurie.com`, Cloudflare Access on, no preview URLs needed; she sets the build command in the dashboard. Proposed by the agent and still to confirm (SPEC §10): workers.dev off, no builds for non-production branches, and treating `wrangler.jsonc` and `public-demo/_headers` as CI for review.

## What is in the repository

| File | Role |
|---|---|
| `wrangler.jsonc` | The Worker: name `culla`, assets only (no script, no bindings), `dist-demo/` as the asset directory, unknown paths answer 404, `workers_dev` and `preview_urls` off. `wrangler deploy` applies these on every deploy. |
| `public-demo/_headers` | Response headers: CSP, `nosniff`, `X-Frame-Options`, `Referrer-Policy`, `Permissions-Policy`, COOP, CORP, HSTS (no preload), and a one-year immutable cache for the hashed files under `/assets/`. Vite copies it into `dist-demo/` in demo mode only (`vite.config.ts`). |
| `tests/demo-site.test.ts` | Fails if a built page (`index.html`, `install.html`) gains an inline script or style, a `style=` or `on…=` attribute, a `data:` URL, or a script or link outside `/assets/`; if any built HTML, CSS or JS holds a `data:` URL (the placeholder photos are SVG files under `/assets/`); if the CSS gains an `@import` or a `url()` other than the two self-hosted font files under `/assets/` (the CSP would block them); if `public-demo/` reaches the desktop build; or if `_headers` loses a header or sets one twice. |

Wrangler is not a dependency of the repository: Workers Builds runs it with `npx`.

## Workers Builds settings

Set these in the Cloudflare dashboard, Workers & Pages, Worker `culla`, Settings.

Order: Margaux sets the build command and the build variables before this PR is merged. Merged first, the push to `main` starts a build with the dashboard defaults, which should fail once and deploy nothing, and the next push after the settings are in builds normally.

| Setting | Value |
|---|---|
| Git repository | `margaux-faurie/culla` |
| Root directory | `/` (the repository root, where `wrangler.jsonc` is) |
| Production branch | `main` |
| Build command | `pnpm install --frozen-lockfile && pnpm build:demo` |
| Deploy command | `npx wrangler deploy` |
| Builds for non-production branches (preview builds) | Off (proposed, to confirm) |
| Build variable `SKIP_DEPENDENCY_INSTALL` | `1`, so the frozen install in the build command is the only install |
| Build variable `NODE_VERSION` | `24`, the major CI uses. The variable pins the major so a change of the Workers Builds default cannot move the build |
| API token | The default build token works. Narrower, optional: a token for this account with Workers Scripts edit, adding only what a failed deploy log asks for |

Domains & Routes (`wrangler.jsonc` declares no routes, so `wrangler deploy` leaves the custom domain set here as it is):

| Setting | Value |
|---|---|
| Custom domain | `culla.margauxfaurie.com` |
| workers.dev | Disabled (also set by `workers_dev: false`; proposed, to confirm) |
| Preview URLs | Disabled (also set by `preview_urls: false`) |

For the zone `margauxfaurie.com`, leave off for this hostname every feature that injects a script into HTML pages: Rocket Loader, email address obfuscation, Web Analytics automatic setup, Zaraz, Bot Fight Mode JavaScript detections. The CSP blocks them, so they would only add console errors.

## Cloudflare Access

An Access application of type self-hosted covers `culla.margauxfaurie.com` (all paths), with the same policy as the rest of the site, until the portfolio opens. Opening the demo to the public is a change to this application, made by Margaux in the dashboard; the repository stays public either way. Preview URLs stay off, and workers.dev is proposed off (to confirm), because those hostnames are outside the Access application.

## Every push to main deploys

Workers Builds deploys the demo on every push to `main`, so a merged PR is live on the demo a few minutes later. Proposed by the agent, to confirm (SPEC §10): changes to `wrangler.jsonc`, `public-demo/_headers` or the Workers Builds settings are deployment changes and wait for Margaux's OK in chat before merge, like CI changes. Nothing enforces this yet: neither `.github/CODEOWNERS` nor the protected-path hook lists these files. Adding `/wrangler.jsonc` and `/public-demo/` to both is a protected-path change, so Margaux makes it.

## Checking a deploy

1. The build log in the dashboard ends with the deployed version and no error.
2. Signed in through Access, `curl -sI https://culla.margauxfaurie.com/` (with the `CF_Authorization` cookie) shows the headers from `_headers`, and a file under `/assets/` shows `Cache-Control: public, max-age=31536000, immutable`.
3. `curl -sI https://culla.margauxfaurie.com/assets/nope.js`, through Access (a browser session or a service token), answers 404 without `Cache-Control: ... immutable`. If a 404 carries the immutable header, a missing asset would be cached for a year: drop `immutable` from `/assets/*` in `_headers` and shorten its `max-age`.
4. Without a session, the same requests are redirected to the Access sign-in page.
5. The browser console on the demo and on `/install` shows no CSP violation from the demo's own files. `pnpm e2e` runs on `vite preview`, which ignores `_headers`: one e2e test (the swipe photo, `e2e/swipe.spec.ts`) injects the CSP read from `_headers` and checks the photo loads with no violation, but every other screen and the other headers run without them, so this manual check stays; `tests/demo-site.test.ts` only checks that the build stays inside the CSP and that `_headers` is well formed.

## Known limits

- `npx wrangler deploy` fetches the latest Wrangler at deploy time, outside the lockfile and the pnpm release-age rule. Pinning a version in the deploy command (`npx wrangler@<version> deploy`) closes that; to decide.
- The build runs repository code (the build scripts and every dependency installed from the lockfile) in Cloudflare's build environment, which holds the build token. If only `main` is built (proposed, to confirm), only merged code runs there; the token's scope bounds what a compromised build can do (threat model T18). The default build token is account-wide, not limited to this Worker.
