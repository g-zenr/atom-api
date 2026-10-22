# Git workflow

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-22
> **Owner:** maintained by the engineering team

How changes reach ATOM: the branches, the commits, the pull requests and the
gates. The environment lifecycle that follows a merge is in
[`release-and-promotion.md`](release-and-promotion.md).

---

## 1. The branch model

Two long-lived branches, and short-lived task branches.

```text
task branch  (feat/ fix/ refactor/ docs/ test/ chore/ ci/ perf/ security/)
      │
      │  pull request + the six gates + review
      ▼
   develop  ──────────────►  DEV environment
      │
      │  UAT acceptance of a specific commit
      ▼
   UAT environment
      │
      │  release approval
      ▼
   PROD environment

master = the production branch; it receives release merges from develop only
```

| Branch | Role | Who writes to it |
| ------ | ---- | ---------------- |
| `master` | The production branch. What PROD runs. | Only a reviewed release merge from `develop`. Never a direct task commit. |
| `develop` | The integration branch. The normal destination of a task. | Pull requests only. |
| task branches | One coherent concern each. | Their author. |

**`develop` and the environments are different things.** DEV, UAT and PROD are
environments, not branches. The project does not maintain long-lived `uat` or
`production` branches: they drift, and the drift is invisible until a release
fails. What moves between environments is an identified commit and, once one
exists, the same build artifact — see [`release-and-promotion.md`](release-and-promotion.md).

## 2. Starting a task

1. **Check the tree.** `git status`. If it is dirty, deal with the existing work
   before starting: never mix an unrelated change into a new branch.
2. **Update `develop`.** `git switch develop && git pull --ff-only`.
3. **Branch from it**, before editing anything:

   ```text
   git switch -c <type>/<short-description>
   ```

4. **Confirm** that `git status --short --branch` shows the new branch.

Never start a task on `develop` itself. Never branch from `master` for a task.

### Branch names

`<type>/<short-description>`, lowercase and hyphen-separated. Include an issue
number when one exists: `<type>/<issue>-<description>`.

| Type | Use |
| ---- | --- |
| `feat/` | A new capability |
| `fix/` | A bug or a regression |
| `refactor/` | Internal restructuring with no intended behaviour change |
| `docs/` | Documentation only |
| `test/` | Tests only |
| `chore/` | Maintenance, tooling, repository configuration |
| `ci/` | The pipeline |
| `perf/` | A performance change **supported by a measurement** |
| `security/` | A security-specific change |

Examples: `fix/f-01-configured-pagination`, `ci/rust-quality-gates`,
`docs/dev-uat-prod-workflow`.

One branch is one coherent concern. If a task needs two, it is two tasks.

## 3. Commits

[Conventional Commits](https://www.conventionalcommits.org/):

```text
<type>(<optional-scope>): <imperative summary>
```

```text
fix(pagination): apply the configured page limits
fix(auth): preserve the absolute refresh session deadline
test(config): cover the rejected security floors
docs(workflow): define the DEV UAT PROD promotion path
ci(github): run the strict documentation check
refactor(config): share the message catalogue fixture
```

- Imperative mood, under 72 characters.
- Explain **why** in the body when it is not obvious.
- One logical change per commit.
- Reference a real issue only: `Refs: #12`, or `Closes: #12` when the pull
  request actually closes it.
- Never `update`, `fixes`, `changes`, `wip`, `final`.
- Never fabricate an issue number, a test result or a sign-off.

**A commit should compile.** When a change is genuinely indivisible — a
signature that several layers share — say so in the commit body rather than
splitting it into commits that do not build. A large honest commit is better
than three broken ones.

Do not rewrite published history. Correct a message with a follow-up commit.

## 4. The gates

A pull request is not ready until all six pass, run from the repository root:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo run --bin atom-docs
cargo run --bin atom-docs -- --strict
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-opencode.ps1
```

The integration tests need a disposable database:

```powershell
$env:TEST_DATABASE_URL = "postgres://atom:atom@127.0.0.1:5432/atom_test"
```

The harness **refuses** a database whose name does not contain `test`. That is a
guard against pointing a test run at real data; do not work around it.

`make check` runs the first three, and `make docs-check` and `make opencode`
run the rest.

Record the **actual** output. Read
[`../testing/verification-policy.md`](../testing/verification-policy.md) for
what "verified" means here: a check that did not run is reported as not run,
never as passing.

## 5. Pull requests

Every change to `develop` arrives as a pull request. The template is
[`.github/pull_request_template.md`](../../.github/pull_request_template.md).

- **Base: `develop`.** Not `master`, not another task branch — unless the pull
  request is explicitly stacked, in which case its base is the branch it stacks
  on and it says so in the body.
- **Title:** a Conventional Commit subject, because the squash merge uses it.
- **Body:** fill in the template honestly. The verification block records real
  results.
- **Size:** keep it reviewable. Split unrelated work.
- Do not self-approve, and do not bypass a required check.

### Merging

The project uses **squash and merge** for task branches: one task branch becomes
one commit on `develop`, and the pull-request title becomes that commit's
message. Release merges (`develop` → `master`) are merge commits, so the release
keeps its history.

After the merge, confirm the merged state and record the merge commit. Delete the
task branch only once its work is fully merged.

## 6. What a review checks

The checklist in the pull-request template is the short version. The reviewer
also checks the rules that are ATOM-specific:

- ownership is still enforced in SQL, with a literal `WHERE user_id = …`, and an
  unowned resource is still `404` rather than `403` (ADR-0005);
- no `.unwrap()` or `.expect()` was added to a request path;
- every user-supplied value is a bound parameter, never string-concatenated SQL;
- a new setting arrived with all seven of its parts (`AGENTS.md` §4 rule 11);
- no secret appears in a file, a log line, an error or a test;
- a claim of "verified" is backed by a recorded command;
- a documentation change accompanies any behaviour change, or the pull request
  says why none was needed.

## 7. Why not trunk-based

A single branch would be simpler, and for a one-person project it would work.
It was not chosen because the release story needs a point that is *always*
deployable (`master`) separate from a point that is *being integrated*
(`develop`), and because the audited remediation volume shows that a review gate
before `develop` catches defects — the independent review of that change found a
blocking shutdown defect and a false "verified" claim that a direct push would
have shipped.

This is a working choice, not a requirement in
[`../project-truth.md`](../project-truth.md). Changing it is an ADR.
