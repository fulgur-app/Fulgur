# Contributing to Fulgur

Thank you for your interest in contributing to Fulgur. This guide covers the essentials: how to build the project, what quality checks are required, and the conventions the codebase follows.

---

## Table of Contents

- [Prerequisites and Building](#prerequisites-and-building)
- [Required Checks Before Submitting](#required-checks-before-submitting)
- [Code Style](#code-style)
- [Documentation](#documentation)
- [Error Handling](#error-handling)
- [Logging](#logging)
- [Tests](#tests)
- [Use of AI / LLM Tools](#use-of-ai--llm-tools)
- [Submitting a Pull Request](#submitting-a-pull-request)

---

## Prerequisites and Building

See the [Build section in README.md](README.md#build) for prerequisites (Rust version, platform-specific requirements) and build commands. That document is the authoritative and up-to-date reference.

---

## Required Checks Before Submitting

Every contribution must pass these three checks, **in order**. Fix each failure before moving to the next.

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

These are enforced by CI. A PR that fails any of them will not be reviewed.

---

## Code Style

- **Language**: All code, comments, variable names, commit messages, and documentation must be in English.
- **Formatting**: Enforced by `rustfmt`. Run `cargo fmt --all` to auto-format before checking.
- **No emojis**: In source code, documentation, or commit messages.
- **Self-documenting code**: Prefer clear, explicit naming over comments. A comment explaining *what* the code does is a sign the code should be renamed or restructured. Only add a comment when the *why* is non-obvious: a hidden constraint, a workaround for a known upstream bug, or a subtle invariant.
- **No over-engineering**: Implement what the task requires. Do not add abstractions, fallbacks, or features for hypothetical future needs.
- **Avoid `unsafe`**: Avoiding `unsafe` is preferred. If `unsafe` is unavoidable, minimize its scope and document why it is necessary.
- **Error handling**: Use `Result` and `Option` for error handling. No `unwrap`, `expect`, or `panic!`. Use `anyhow` for error types or enums when appropriate.
- **Avoid duplicated code**: If you find yourself copying and pasting code, consider refactoring into a shared function or macro.

---

## Documentation

Every public function and non-trivial private function must have a documentation comment using Rust's `///` syntax with the following structure:

```rust
/// Short summary line
///
/// ### Description
/// Optional — only when the summary alone is not enough to understand the behavior.
///
/// ### Arguments
/// - `arg_name`: What it is and any constraints
///
/// ### Returns
/// - `Ok(T)`: What success looks like
/// - `Err(E)`: What can fail and why
pub fn share_file(request: ShareFileRequest) -> anyhow::Result<(T)> {
    ...
}
```

Omit `### Description`, `### Arguments`, or `### Returns` when they add no information (e.g. a function with no arguments, or one whose name and return type are self-explanatory). Do not describe the implementation — describe the contract.

---

## Error Handling

Wrap errors with context using `anyhow`. Always include the underlying error to preserve diagnostic detail:

```rust
// Correct
serde_json::from_str(&raw).map_err(|e| anyhow!("failed to parse settings: {}", e))?;

// Wrong — loses the underlying error message
serde_json::from_str(&raw).map_err(|_| anyhow!("failed to parse settings"))?;
```

This applies to all JSON parsing, I/O, and external library calls.

---

## Logging

Use the `log` crate macros exclusively. Never use `println!` or `eprintln!` in application code.

```rust
log::info!("settings loaded from {}", path.display());
log::warn!("file watcher event ignored: {:?}", event);
log::error!("failed to save state: {}", err);
```

The only exception is bootstrap code that runs before the logger is initialized (e.g., in `main.rs` before `init_logger()` is called), where `eprintln!` is acceptable.

---

## Tests

Integration tests live in the `tests/` directory. Unit tests live inline in their source file using `#[cfg(test)]` modules.

Guidelines:

- Use the `tempfile` crate for temporary directories. Never write test data to fixed paths.
- Always canonicalize paths before comparing them on macOS, which resolves `/var/` to `/private/var/`:
  ```rust
  assert_eq!(path.canonicalize()?, expected.canonicalize()?);
  ```
- File watcher tests require at least 500 ms of initialization time and should use timeouts of 5 seconds or more for event detection.
- Do not mock I/O in integration tests. Test against real files and real state.
- Except for platform specific features, tests should run on all platforms.

---

## Branch Naming

All contribution branches must follow the pattern:

```
dev-<short-description>
```

Use lowercase kebab-case for the description. Keep it short but specific enough to identify the work.

```
dev-markdown-preview
dev-file-watcher-debounce
dev-perf-tab-bar
dev-fix-share-sheet
```

Do not use `feature/`, `fix/`, or other prefixes. The `dev-` prefix is the project convention.

---

## Commit Messages

Follow the [Conventional Commits](https://www.conventionalcommits.org/) specification. The format is:

```
<type>(<scope>): <description>
```

**Types:**

| Type | When to use |
|---|---|
| `feat` | New feature or behavior |
| `fix` | Bug fix |
| `perf` | Performance improvement |
| `refactor` | Code restructuring without behavior change |
| `test` | Adding or updating tests |
| `chore` | Build scripts, version bumps, config changes |
| `docs` | Documentation only |

**Scope** is optional but recommended. Use the module or area affected, in kebab-case:

```
feat(sync): add device revocation flow
fix(file-watcher): handle rapid successive saves correctly
perf(tab-bar): precompute filename counts for title disambiguation
chore(build): update icon for Linux installer
```

**Description rules:**
- Lowercase, no trailing period
- Imperative mood ("add", "fix", "remove") not past tense ("added", "fixed")
- Under 72 characters for the subject line
- For non-obvious changes, add a body after a blank line explaining the *why*, not the *what*

---

## Use of AI / LLM Tools

You are welcome to use AI assistants (e.g. Claude, Copilot, ChatGPT) during development. However, all LLM-generated code must be reviewed, understood, and validated by the developer before being submitted. Submitting code you cannot explain or defend is not acceptable.

In practice this means:

- Run all [required checks](#required-checks-before-submitting) on the generated code, as you would for any other code.
- Verify that generated code follows the conventions in this document (naming, error handling, documentation, logging).
- Do not submit generated code verbatim if it introduces patterns inconsistent with the rest of the codebase.

You are the author of what you submit, regardless of how it was produced. Low effort slop will not be tolerated and may result in a ban.

---

## Submitting a Pull Request

1. Fork the repository and create a branch from `main`.
2. Make your changes following the conventions above.
3. Run the [required checks](#required-checks-before-submitting) and fix any failures.
4. Open a pull request with a clear title and a description of what changed and why.
5. Link any relevant issue in the PR description.

For significant changes, open an issue first to discuss the approach before investing time in an implementation.
