---
name: Atom Specialist
description: Use when the task needs a role the standing team does not cover — CI/CD, performance, dependency supply chain, migration review, frontend, automation integration. Defines how to add a temporary specialist safely.
---

# Adding a specialist agent

The standing team covers the ATOM backend. When a task genuinely needs a
different lens, add a **temporary specialist** rather than stretching an
existing role or doing the work yourself.

Do this only when the standing team cannot cover the task. A one-off question
about a Rust iterator is not a reason to create an agent.

## When to add one

| Situation                                      | Specialist to add                  |
| ---------------------------------------------- | ---------------------------------- |
| A CI pipeline or container change needs review | CI/CD and release engineer         |
| A query or hot path is too slow                | Performance engineer               |
| A dependency bump or a new dependency          | Supply-chain auditor               |
| A migration must be reviewed for data loss     | Migration reviewer                 |
| A public response shape changed                | API compatibility reviewer         |
| A future client (web, mobile) contract is designed | Frontend or client engineer    |
| An n8n or other automation integration is designed | Automation integration engineer |

## How to add one

1. Write the agent file to `.opencode/agents/<specialist-name>.md`. The file
   name becomes the agent id.
2. Use the frontmatter fields below. The body is the system prompt.
3. Keep it short and specific to the lens, not a copy of a generalist.
4. Give it the narrowest permissions it needs. A reviewer is read-only.
5. Tell the orchestrator to delete the file when the task is done, unless the
   role is worth keeping.

### Frontmatter template

```yaml
---
description: <one line: what it does and when to use it>
mode: subagent
color: warning
permissions:
  - action: subagent
    resource: "*"
    effect: deny
  - action: question
    resource: "*"
    effect: deny
  # add only the restrictions this role needs
---
```

## Rules for the specialist

- It may only edit the files its brief assigns. No two agents own the same file
  in the same wave.
- It must not change the product requirements. It reports a conflict, it does
  not resolve it by editing `docs/project-truth.md`.
- It must report the checks it actually ran.
- It must not modify the implementation it reviews.

## Prompt structure

```text
You are the <role> for ATOM.

Objective      <one sentence>
Scope          <files or modules you own; nothing else>
Out of scope   <what you must not touch>
Context        <the accepted requirements and contracts that apply>
Method         <how to approach the work>
Acceptance     <how the orchestrator will judge the result>
Verification   <the commands you must run>
Prohibited     <destructive or out-of-bounds operations>
Output         <what your report must contain>
```

## After the task

- Remove the temporary agent file, or move it to a permanent role with a
  written justification in the change log.
- Record what the specialist found in `docs/engineering/review-log.md` if it
  produced findings.
