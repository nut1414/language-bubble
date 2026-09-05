# Agent execution and review contract

[Execution map](README.md) · [Validation](00-validation.md)

Give an external agent the repository checkout and these documents together. Relative links are portable within the repository. Read actual source again: plans refer to an inspected baseline and may become stale.

## Executor prompt

```text
Execute refactor packet <R-ID> in docs/refactor/<packet-file>.md.
Read docs/refactor/README.md and 00-validation.md first, plus applicable AGENTS.md.
Report the current git commit/status and verify prerequisite packets are integrated.
Preserve all pre-existing work, particularly Package.appxmanifest.
Stay within the packet's file ownership. Keep current public interfaces until
the integrator coordinates call-site changes. Use native git, not gh CLI.
Perform numbered checkpoints in order. Characterize observable behavior before
moving it. Separate semantic fixes from mechanical refactoring.
Do not publish, tag, bump versions, upload, or modify real user registry settings
for tests. Do not introduce dependencies without explaining necessity.
For shared files, deliver a proposed patch to the integrator instead of racing
other agents. Do not spawn further agents unless the coordinator requests it.
Run the packet's targeted checks and common gates. Record exact commands,
exit codes, manual evidence, and unavailable checks. Never claim an unrun test.
Return the completion report below. Stop short of declaring completion if a
required gate remains unavailable or fails; identify the precise remaining work.
```

## Reviewer prompt

```text
Independently review packet <R-ID>, its implementation diff, and reported evidence.
First inspect the base commit and confirm the plan still matches source.
Check scope, behavior preservation, unsafe pointer/resource lifetime, callback
reentrancy, error paths, and compatibility requirements from the packet.
Inspect test assertions for behavioral coverage, not merely moved code coverage.
Check that failures and unavailable manual checks are disclosed.
Return actionable findings with severity, file/symbol, reproduction or reasoning,
and smallest corrective action. Distinguish demonstrated defects from questions.
Do not edit code during review. Report no findings when justified, but state
remaining validation limitations. Do not approve on compilation alone.
```

## Completion report

```markdown
# R-ID completion report
- Base commit and pre-existing changes:
- Resulting commit(s), or patch location if uncommitted:
- Completed checkpoints:
- Changed files and reason:
- Preserved behavior and any proposed separate fix:
- Exact commands, exit codes, target architecture:
- Manual cases exercised and evidence:
- Checks not run and why:
- Reviewer findings and resolutions:
- Shared-file integration required:
- Rollback commit(s) and dependent packets:
- Status: complete / needs changes / blocked validation
```

The coordinator updates the [tracker](README.md#completion-tracker) after accepting evidence. An external agent's assertion is input to review, not a substitute for it.
