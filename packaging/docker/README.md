# Neoism Workspace Daemon Container

The image runs only `neoism-workspace-daemon` on port `9876` and stores
mutable data under `/var/lib/neoism`.

Important env vars:

- `NEOISM_CLOUD_PROVISION_TOKEN`: bearer token accepted by `POST /workspace/from-git`.
- `NEOISM_DAEMON_TOKEN`: legacy websocket/provision bearer token.
- `NEOISM_REQUIRE_AUTH=1`: require a valid pairing/device/daemon token during `Hello`.
- `NEOISM_WORKSPACES_DIR=/var/lib/neoism/workspaces`: clone target root.

Provision a git workspace:

```sh
curl -fsS \
  -H "Authorization: Bearer $NEOISM_CLOUD_PROVISION_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"git_url":"https://github.com/owner/repo.git","ref":"main"}' \
  http://HOST:9876/workspace/from-git
```

The response contains the registered workspace summary. Reposting the
same repo reuses the existing directory and runs a fast-forward update
unless `"pull": false` is supplied.
