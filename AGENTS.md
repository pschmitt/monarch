# Agent instructions

Monarch is an open-source dashboard for monitoring Monit agents. The repository includes a Rust backend and a web frontend; keep API and UI changes consistent across both.

## Working and validation

- Follow the README and project-specific instructions for the backend or frontend area you change.
- Run focused Rust and web checks using the repository's documented commands; avoid unrelated dependency or generated-file changes.
- Before changing GitHub Actions, check whether [pschmitt/ci-workflows](https://github.com/pschmitt/ci-workflows) provides a reusable workflow or action. Preserve current behavior and check coverage, and use `@main` for shared workflow or action references.
