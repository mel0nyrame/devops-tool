# Issue tracker: GitHub

Specs and tickets live in GitHub Issues for the repository configured as `origin`.
Before writing, verify the target with `gh repo view --json nameWithOwner`.

Use `gh issue create` to publish, `gh issue view` to read, and
`gh issue list` to find tickets. Use native GitHub sub-issues and blocking
dependencies when available. Otherwise, record `Part of #<number>` or
`Blocked by: #<number>` in the issue body.

Use the [triage label mapping](triage-labels.md) when changing labels.

**PRs as a request surface: no.**
