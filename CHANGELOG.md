# Changelog

All notable changes to apic are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-06-09

First crates.io release. Adds interactive resolution, tree output, project
templates, an editor flag, and a `remove` command on top of the betas.

### Added
- `list` renders contracts as a box-drawing tree on terminals, with fuzzy-match
  highlighting; piped/non-TTY output stays flat for scripts.
- Interactive picker that prompts you to choose when a contract name is ambiguous,
  wired into `read`, `open`, and `validate -f`.
- `--editor <cmd>` flag to choose the editor per invocation, replacing the editor
  setting in config.
- `.apic/template.json` is seeded on `init` and used by `apic create`; supports
  partial template merge so you only override the fields you set.
- `open --template` to open the project template directly.
- `remove` command to delete a contract.
- `init` now recovers a partially-initialized project instead of erroring.

### Changed
- Contract `url` restructured into a `url` object (base + path + query + variables).
- `init` template seeding is best-effort and no longer blocks initialization.

## [0.1.0-beta.2] - 2026-06-04

### Added
- `read --example` and example payloads rendered beneath the schema tables.

### Changed
- Dropped the `author` field from crate metadata.

## [0.1.0-beta.1] - 2026-06-04

Initial beta release.

### Added
- Core commands: `init`, `config`, `create`, `list`, `read`, `validate`, `open`.
- Contracts stored as plain per-endpoint JSON files, designed to be diffed and
  reviewed in git.
- Security hardening: path confinement, file-size cap, output sanitization.
- SIGPIPE handling so piped output (e.g. `apic read | head`) exits cleanly.
- CI (fmt, clippy, build, test) and unit + end-to-end test suites.
- MIT license.

[Unreleased]: https://github.com/rizukirr/apic/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/rizukirr/apic/compare/v0.1.0-beta.2...v0.1.0
[0.1.0-beta.2]: https://github.com/rizukirr/apic/compare/v0.1.0-beta.1...v0.1.0-beta.2
[0.1.0-beta.1]: https://github.com/rizukirr/apic/releases/tag/v0.1.0-beta.1
