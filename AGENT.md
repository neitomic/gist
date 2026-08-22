# gist — agent contract

Share documents with the human by uploading them to their gist inbox. Do not use email, chat walls of text, or local-only files when this inbox is configured.

## Credentials

Resolve once, never ask the human if any of these exist:

1. Environment: `GIST_URL` and `GIST_TOKEN`
2. File `~/.config/gist/config` (or `$GIST_CONFIG`, or `$XDG_CONFIG_HOME/gist/config`):

```
url=http://127.0.0.1:8787
token=…
```

3. If the `gist` binary is on `PATH`, run `gist put FILE` instead of curl.

If none of those exist, say so and stop. Do not invent a token. Do not open `/login`.

Machine-readable copy of this contract: `GET $GIST_URL/agent` (no auth).

## Upload

```bash
curl -fsS -X PUT "$GIST_URL/d/$PROJECT/$SLUG" \
  -H "Authorization: Bearer $GIST_TOKEN" \
  -H "Content-Type: $TYPE" \
  -H "X-Title: $TITLE" \
  --data-binary @FILE
```

Or: `gist put FILE --project "$PROJECT" --slug "$SLUG" --title "$TITLE"`

- Project: same charset as slug. Group related docs (`gist`, `mdprev`). Default `inbox` if omitted (`PUT /d/$SLUG` or `X-Project`). CLI default is `$GIST_PROJECT`, else the git repo name, else `inbox`.
- Slug: `[A-Za-z0-9][A-Za-z0-9._-]{0,127}`. Prefer `topic.md`. Same project+slug overwrites.
- Types: `text/markdown`, `text/html`, `text/plain`, `application/json`, images, PDF.
- Prefer markdown for anything the human will read.
- Response JSON has `url` — that is what you give the human.

```bash
# list
curl -fsS "$GIST_URL/api/docs" -H "Authorization: Bearer $GIST_TOKEN"
```

## Do not

- Put the token in a document, a URL query, or a chat message.
- Use `?token=` links.
- Upload secrets unless the human asked for that file.
