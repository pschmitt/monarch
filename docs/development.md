# Development

```sh
nix develop
(cd web && npm ci && npm run dev)      # UI on :5173, proxies /api to :8080
cargo run                              # backend on :8080
(cd web && VITE_MOCK=1 npm run dev)    # UI with mock data, no backend needed
cargo test
(cd web && npm run check)              # svelte-check
nix flake check                        # includes a NixOS VM test with a real Monit agent
```

The HTTP API used by the UI is described in [API.md](API.md).
