# Design and delivery checkpoints

## Design interview

- For a source-layout decision, show two shallow trees. Ask about package boundaries separately from folder placement.
- Look up bounded facts directly. Give a delegate only an independent evidence question; the agent writing the decision owns its citations. Put small, decision-bound research in the owning ADR when authorized.
- Before implementation, state the agreed design, what will change, and what is excluded. Wait for explicit permission to implement. Approval to record a decision or term is not approval to change source.

## First push to a remote

1. Stage explicit paths. Inspect the complete staged diff in a separate step before committing. Run `git diff --cached --check`. If you scan for secrets, accept only the scanner's clean result, not an execution error.
2. Confirm the branch and outgoing commits. Compare `git remote get-url --push origin` with the approved destination. For a private GitHub repository, inspect `gh repo view OWNER/REPO --json nameWithOwner,visibility` for that exact destination before pushing.
3. After pushing, identify CI by the pushed commit SHA. If no matching run appears after a bounded wait, report that the push succeeded and CI is not yet observed.
