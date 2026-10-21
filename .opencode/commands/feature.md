---
description: Start a feature with the full ATOM delivery workflow.
---

Start a new feature on the ATOM backend.

Load the `atom-delivery` skill and follow it end to end.

Feature name: $ARGUMENTS

Work through the stages in order:

1. **Intake** — read `AGENTS.md`, `docs/README.md`, `docs/project-truth.md`,
   the related feature documents, the ADRs, the known issues and the open
   questions. Inspect the real code.
2. **Define** the task in the standard shape: outcome, requirements,
   existing implementation, constraints, code changes, documentation changes,
   verification criteria, conflicts.
3. **Classify the risk** — routine, contract-affecting, security-sensitive,
   data-changing or cross-module. State the class and what it implies.
4. **Design** if the class requires it: delegate to the architect first and
   agree the contracts before any implementation starts.
5. **Delegate** the implementation to the specialist that owns the files.
   Give every brief the objective, scope and owned files, dependencies and
   contracts, acceptance criteria, expected output, required verification,
   whether the agent may edit, and the prohibited operations.
6. **Verify** with the real commands.
7. **Review** with the read-only `code-reviewer`, then fix what it finds.
8. **Gate** with the read-only `quality-gate` until it returns `PASS`.
9. **Document** with the `atom-docs` skill.
10. **Report** the outcome, the changed files, the verification evidence, the
    documentation changes and the remaining limitations.

Do not stop at a plan. Continue to implementation, verification and review.
Report a blocker only when it is genuine.
