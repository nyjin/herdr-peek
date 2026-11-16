# herdr-peek

peek, a herdr plugin: a file or link selected or Ctrl+clicked in a herdr pane, rendered by macOS's WebKit and shown
in the pane beside it with Kitty graphics. One Rust crate; the repository root is the plugin (`herdr-plugin.toml`).

## Layout

- `src/`: platform-neutral modules (build and test anywhere); `src/mac/`: WebKit renderer, viewer, FSEvents, Keychain,
  pasteboard (macOS only).
- `tests/`: `cargo test`, including `tests/language.rs` (the language rule below).
- `bin/peek`: runs `target/release/peek` from the checkout.
- `evals/`: the rendering spike and integration test records; `evals/screens/` stays local (git-ignored: real,
  partly private documents).
- `docs/superpowers/specs/`, `docs/superpowers/plans/`: dated records. When behaviour changes, update the current
  design doc (`docs/superpowers/specs/2026-11-15-peek-webkit-viewer-design.md`), not old plans.
- History: peek lived in nyjin/herdr-skills (`herdr-plugins/peek`, earlier `herdr-plugins/glow-preview`) until
  2026-11-17; this repository carries the `herdr-plugins/peek` history.

## Language

Everything in the repository is English: code, comments, UI strings, docs, commit messages. Korean appears only in
`*.ko.md` files and in tests where it does a job (sample content, patterns), with an English comment saying why.
`cargo test` fails otherwise.

## Checks

Run `cargo test` before committing. CI (`.github/workflows/check.yml`) runs it on Linux and macOS for branches and
pull requests.

## Releases

semantic-release runs on every push to `main` (`.github/workflows/release.yml`, `.releaserc.json`). Commit messages
follow Conventional Commits and decide the version: `feat` minor, `fix` and `perf` patch, a breaking change major;
other types make no release. It writes the version into `Cargo.toml` and `herdr-plugin.toml`
(`scripts/set-version.sh`), commits it back as `chore(release): X.Y.Z [skip ci]`, tags `vX.Y.Z` and publishes a
GitHub release with `peek-macos-aarch64` and its SHA-256. Do not edit the versions by hand. The tag `v0.0.0` on the
initial commit is where counting starts, so the first release is 0.1.0.

## Worktrees

Agent worktrees go under `.worktrees/<name>` (git-ignored), one per task.
