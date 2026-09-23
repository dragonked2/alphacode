# OAuth Login

AlphaCode signs into providers with OAuth instead of pasting API keys wherever
possible. Run `/login` (or `alphacode login`) and a browser window opens; after
you approve, the provider redirects back to a callback URL and the CLI captures
the authorization code automatically.

## Callback URLs

- **OpenAI (Codex OAuth):** `http://localhost:1455/auth/callback`

  The CLI listens on port `1455` on localhost during login. If that port is
  busy, login fails — free it (or stop the process holding it) and retry.

- **Claude (platform.claude.com OAuth):** `https://platform.claude.com/oauth/code/callback`

  (Legacy console callback: `https://console.anthropic.com/oauth/code/callback`.)

## Troubleshooting

- **Browser opens but login never completes:** VPNs, firewall rules, browser
  policies, or local networking configuration can block the authentication
  callback. Make sure `localhost` resolves and nothing intercepts local
  connections, then retry `/login`.
- **Port already in use (OpenAI):** another program is listening on port
  `1455`. Stop it and run `/login` again.
- **State mismatch errors:** never reuse a login URL — always start a fresh
  `/login` so the one-time state token matches.
