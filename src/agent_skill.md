# Drop a document to gist

The user has a token-gated inbox. Upload the file; give them the `url` from the response. Do not paste long documents into chat when this works.

## Credentials

Never ask for the token if any of these exist:

1. `$GIST_URL` and `$GIST_TOKEN`
2. `~/.config/gist/config` (`url=` and `token=`)
3. The `gist` binary — then run `gist put FILE`

If none exist, say gist is not configured and stop. Do not invent a token.

## Upload

Prefer the CLI:

```bash
gist put FILE.md --project repo-name --slug topic.md --title "Short title"
```

`--project` defaults to `$GIST_PROJECT`, then the git repo folder name, then `inbox`.

Otherwise:

```bash
curl -fsS -X PUT "$GIST_URL/d/repo-name/topic.md" \
  -H "Authorization: Bearer $GIST_TOKEN" \
  -H "Content-Type: text/markdown" \
  -H "X-Title: Short title" \
  --data-binary @FILE.md
```

Slug: `[A-Za-z0-9][A-Za-z0-9._-]+`. Same project+slug overwrites. Prefer `.md`.

## Do not

- Put the token in a document, a URL query, or a chat message.
- Use `?token=` links or open `/login`.
- Upload secrets unless the human asked for that file.

Reply with the returned `url` only.
