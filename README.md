# gist

A small token-gated drop box. Agents `PUT` markdown, HTML, text, images, or PDFs. You read them in a browser.

Works on a laptop (`127.0.0.1`) or behind TLS on a server. Documents live as files. No database.

## Run locally

```bash
export GIST_TOKEN=$(openssl rand -hex 24)
cargo run --release
```

Open http://127.0.0.1:8787 and sign in with the token. The browser can save it as a site password. After that it stays signed in for up to 400 days via a cookie.

If `GIST_TOKEN` is unset, a token is generated and written to `data/.token` (mode `0600`).

## Agents

The server writes `~/.config/gist/config` (mode `0600`) on startup. Local agents should never log in through the browser.

```bash
# after the server has been started once
gist put standup.md                    # project = git repo name, or inbox
gist put standup.md --project gist
gist put report.html --title "Q3"
gist list
gist env          # print export GIST_URL / GIST_TOKEN for a remote agent
```

Credential order: `GIST_URL` + `GIST_TOKEN` in the environment, then `~/.config/gist/config`.

Public contract (no token in the response):

- `GET /agent` — JSON
- `GET /agent.md` — this protocol

If the `gist` binary is not on `PATH`, agents `PUT $GIST_URL/d/{slug}` with `Authorization: Bearer $GIST_TOKEN`. Full contract: [AGENT.md](AGENT.md).

Install the binary somewhere on `PATH`:

```bash
cargo install --path .
```

## Agent API

Same token for write and read. Send it as `Authorization: Bearer …` (or `X-Gist-Token`).

```bash
# Markdown (rendered in the inbox)
curl -fsS -X PUT "$GIST_URL/d/standup.md" \
  -H "Authorization: Bearer $GIST_TOKEN" \
  -H "Content-Type: text/markdown" \
  --data-binary @standup.md

# HTML (sanitized before display)
curl -fsS -X PUT "$GIST_URL/d/report.html" \
  -H "Authorization: Bearer $GIST_TOKEN" \
  -H "Content-Type: text/html" \
  --data-binary @report.html

# Plain text / JSON / anything else
curl -fsS -X PUT "$GIST_URL/d/trace.json" \
  -H "Authorization: Bearer $GIST_TOKEN" \
  -H "Content-Type: application/json" \
  --data-binary @trace.json

# Optional title; omit the slug and one is generated
curl -fsS -X POST "$GIST_URL/api/docs" \
  -H "Authorization: Bearer $GIST_TOKEN" \
  -H "Content-Type: text/markdown" \
  -H "X-Title: Weekly review" \
  --data-binary @review.md
```

`PUT` to the same slug overwrites. Response:

```json
{
  "slug": "standup.md",
  "title": "Standup",
  "url": "http://127.0.0.1:8787/d/standup.md",
  "raw_url": "http://127.0.0.1:8787/d/standup.md/raw"
}
```

| Method | Path | What |
| --- | --- | --- |
| `PUT` | `/d/{slug}` | Create or replace a document |
| `GET` | `/d/{slug}` | Rendered view (browser) |
| `GET` | `/d/{slug}/raw` | Original bytes |
| `DELETE` | `/api/docs/{slug}` | Remove |
| `GET` | `/api/docs` | JSON list |
| `GET` | `/health` | Liveness, no auth |

Slugs are `[A-Za-z0-9][A-Za-z0-9._-]{0,127}`. No paths, no `..`.

Markdown is converted to HTML with GitHub-flavored extras (tables, task lists, footnotes, alerts), a table of contents, and server-side syntax highlighting (theme picker on the doc page). Fenced `mermaid` blocks render in the browser. Uploaded HTML is passed through [ammonia](https://docs.rs/ammonia) so scripts and event handlers from the document do not run. Raw HTML/JS/SVG is served as `text/plain`.

## Environment

| Variable | Default | Meaning |
| --- | --- | --- |
| `GIST_TOKEN` | generated into `data/.token` | Shared secret, min 16 chars |
| `GIST_BIND` | `127.0.0.1:8787` | Listen address |
| `GIST_DATA` | `./data` | Document directory |
| `GIST_MAX_BYTES` | `8388608` (8 MiB) | Upload cap |
| `GIST_PUBLIC_URL` | inferred from `Host` | Public origin, e.g. `https://gist.example.com` |

## Run on a server

Bind on localhost and terminate TLS in Caddy or nginx. Do not expose the process on the public internet without HTTPS.

```bash
export GIST_TOKEN=…          # long random value
export GIST_BIND=127.0.0.1:8787
export GIST_DATA=/var/lib/gist
export GIST_PUBLIC_URL=https://gist.example.com
```

Caddy:

```caddyfile
gist.example.com {
    reverse_proxy 127.0.0.1:8787
}
```

Docker (this machine):

```bash
docker build -t gist .
docker run --rm -p 127.0.0.1:8787:8787 \
  -e GIST_TOKEN="$GIST_TOKEN" \
  -e GIST_BIND=0.0.0.0:8787 \
  -v gist-data:/data \
  gist
```

linux/amd64 and linux/arm64 (buildx). A multi-arch image has to be pushed to a registry; `--load` only works for one platform:

```bash
docker buildx create --use --name gist
docker buildx build --platform linux/amd64,linux/arm64 \
  -t ghcr.io/neitomic/gist:latest --push .
# or without pushing: docker buildx bake
```

Pushes to `main` publish `ghcr.io/neitomic/gist` for both architectures.

Inside the container the process listens on all interfaces; keep the published port on `127.0.0.1` or behind a proxy.

## Security model

- Every read and write except `/health` needs the token (header, `X-Gist-Token`, or HttpOnly cookie after login).
- Tokens are compared in constant time.
- Body size is capped.
- Slugs cannot escape the data directory.
- HTML is sanitized. User documents cannot run script.
- The document viewer loads a small first-party script (`/static/doc.js`) for the code-theme picker, copy buttons, and TOC highlighting. Mermaid diagrams pull `mermaid.min.js` from jsDelivr. CSP allows only those script sources.

This is a personal inbox, not a multi-user product. Treat the token like a password.
