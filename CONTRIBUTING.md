# Contributing

Thank you for your interest in contributing to CryoDB.

## Before You Start

Please:

* Search existing issues before opening a new one.
* Check open pull requests before implementing large changes.
* Open a discussion for major architectural modifications.

## Reporting Bugs

When reporting a bug, include:

* Operating system
* CryoDB version
* Renderer backend
* Steps to reproduce
* Expected behavior
* Actual behavior

## Code Style

* Prefer readability over cleverness.
* Prefer simplicity over abstraction.
* Keep modules focused and cohesive.
* Avoid unnecessary allocations.
* Avoid code duplication.
* Favor explicit behavior over hidden behavior.
* Follow existing project patterns whenever possible.

### Comments

Comments should be rare.

Prefer clear naming and code structure over explanatory comments.

Use comments only when documenting:

* Architectural decisions
* External limitations
* Non-obvious behavior
* Performance tradeoffs

### AI-Assisted Contributions

AI-assisted contributions are welcome.

However, contributors are responsible for understanding, reviewing, testing, and validating all submitted code.

**Do not submit code that you do not understand.**

### Pull Requests

Before submitting a pull request:

* Ensure the project builds successfully.
* Remove debug and temporary code.
* Remove commented-out code.
* Keep changes focused and minimal.
* Add a `CHANGELOG.md` entry if users would notice the change.

## Changelog

`CHANGELOG.md` is compiled into the binary and rendered by the in-app "What's
new" modal after an update, so it is part of the product, not release paperwork.

If your change is visible to users — a new feature, changed behavior, a fixed
bug, a removed option — add one line under `## [Unreleased]`, in the right
`### Added` / `### Changed` / `### Fixed` / `### Removed` group, in the same
commit. Refactors, tests and CI changes need no entry.

Write for users. *"Folders can be renamed"*, not *"refactor folder store"*.

Maintainers release by promoting `[Unreleased]` to `## [x.y.z] - YYYY-MM-DD`,
bumping `Cargo.toml`, and pushing a `vX.Y.Z` tag. `cargo test changelog::` fails
when the running version has no section.

## Architecture

Before making significant changes, review:

* `docs/ARCHITECTURE.md`

## Licensing

By contributing to CryoDB, you agree that your contributions will be licensed under the same license as the project.

## Community

Be respectful and constructive.

The goal is to build a database toolkit that remains maintainable for many years and welcomes contributors of all experience levels.
