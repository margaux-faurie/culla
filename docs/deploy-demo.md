# Deploying the web demo

Status: 27 Nov 2026. The web demo is the `src/` build in demo mode (ADR 0014), served as static assets by a Cloudflare Worker named `culla` at `culla.margauxfaurie.com`, behind Cloudflare Access until the portfolio opens (SPEC §2). Workers Builds builds and deploys it from this repository.

## What is in the repository

| File | Role |
|---|---|
| `wrangler.jsonc` | The Worker: name `culla`, assets only (no script, no bindings), `dist-demo/` as the asset directory, unknown paths answer 404, `workers_dev` and `preview_urls` off. `wrangler deploy` applies these on every deploy. |
| `public-demo/_headers` | Response headers: CSP, `nosniff`, `X-Frame-Options`, `Referrer-Policy`, `Permissions-Policy`, COOP, CORP, HSTS (no preload), and a one-year immutable cache for the hashed files under `/assets/`. Vite copies it into `dist-demo/` in demo mode only (`vite.config.ts`). |
| `tests/demo-site.test.ts` | Fails if the built demo gains an inline script or style or a `data:` URL in its CSS (the CSP would block them), if `public-demo/` reaches the desktop build, or if `_headers` loses a header or sets one twice. |

Wrangler is not a dependency of the repository: Workers Builds runs it with `npx`.

## Workers Builds settings

Set these in the Cloudflare dashboard, Workers & Pages, Worker `culla`, Settings.

| Setting | Value |
|---|---|
| Git repository | `margaux-faurie/culla` |
| Root directory | `/` (the repository root, where `wrangler.jsonc` is) |
| Production branch | `main` |
| Build command | `pnpm install --frozen-lockfile && pnpm build:demo` |
| Deploy command | `npx wrangler deploy` |
| Builds for non-production branches (preview builds) | Off |
| Build variable `SKIP_DEPENDENCY_INSTALL` | `1`, so the frozen install in the build command is the only install |
| Build variable `NODE_VERSION` | `24`, the version CI uses |
| API token | The default build token works. Narrower, optional: a token for this account with Workers Scripts edit, adding only what a failed deploy log asks for |

Domains & Routes (`wrangler.jsonc` declares no routes, so `wrangler deploy` leaves the custom domain set here as it is):

| Setting | Value |
|---|---|
| Custom domain | `culla.margauxfaurie.com` |
| workers.dev | Disabled (also set by `workers_dev: false`) |
| Preview URLs | Disabled (also set by `preview_urls: false`) |

For the zone `margauxfaurie.com`, leave off for this hostname every feature that injects a script into HTML pages: Rocket Loader, email address obfuscation, Web Analytics automatic setup, Zaraz, Bot Fight Mode JavaScript detections. The CSP blocks them, so they would only add console errors.

## Cloudflare Access

An Access application of type self-hosted covers `culla.margauxfaurie.com` (all paths), with the same policy as the rest of the site, until the portfolio opens. Opening the demo to the public is a change to this application, made by Margaux in the dashboard; the repository stays public either way. Workers.dev and preview URLs stay off, because those hostnames are outside the Access application.

## Every push to main deploys

Workers Builds deploys the demo on every push to `main`, so a merged PR is live on the demo a few minutes later. Changes to `wrangler.jsonc`, `public-demo/_headers` or the Workers Builds settings are deployment changes: they wait for Margaux's OK in chat before merge, like CI changes (SPEC §2, review and merge).

## Checking a deploy

1. The build log in the dashboard ends with the deployed version and no error.
2. Signed in through Access, `curl -sI https://culla.margauxfaurie.com/` (with the `CF_Authorization` cookie) shows the headers from `_headers`, and a file under `/assets/` shows `Cache-Control: public, max-age=31536000, immutable`.
3. Without a session, the same request is redirected to the Access sign-in page.
4. The browser console on the demo shows no CSP violation.

## Known limits

- `npx wrangler deploy` fetches the latest Wrangler at deploy time, outside the lockfile and the pnpm release-age rule. Pinning a version in the deploy command (`npx wrangler@<version> deploy`) closes that; to decide.
- The build runs repository code (the build scripts and every dependency installed from the lockfile) in Cloudflare's build environment, which holds the build token. Only `main` is built, so only merged code runs there; the token's scope bounds what a compromised build can do (threat model T18). The default build token is account-wide, not limited to this Worker.
