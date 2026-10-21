# Using OpenCode in ATOM

> **Status:** Accepted (living document)
> **Last updated:** 2026-10-21
> **Owner:** maintained by the `docs` agent

This guide is for contributors who want to drive the ATOM backend with
OpenCode, and for anyone who needs to change the committed configuration.

- The agent team and the delivery workflow are described in
  [`project-truth.md`](project-truth.md) and
  [`../AGENTS.md`](../AGENTS.md).
- The repository instructions every agent reads are
  [`../AGENTS.md`](../AGENTS.md).
- The human contributor guide is [`../CONTRIBUTING.md`](../CONTRIBUTING.md).

---

## 1. What is committed, and what is yours

| File | Committed? | Purpose |
| ---- | ---------- | ------- |
| `opencode.jsonc` | yes | The shared baseline: default model, permissions, policies, ambient instructions, watcher and compaction settings. |
| `AGENTS.md` | yes | The instructions every agent reads. |
| `.opencode/agents/*.md` | yes | The agent team. |
| `.opencode/commands/*.md` | yes | Reusable slash commands. |
| `.opencode/skills/*/SKILL.md` | yes | Reusable workflows. |
| `.opencode/opencode.jsonc` | **no** (git-ignored) | Your personal override: model, extra permissions, extra instructions. |
| `~/.config/opencode/opencode.jsonc` | no | Your machine-wide OpenCode configuration. |

**Never put an API key, a token or a personal path in a committed file.**
Provider credentials belong in the environment or in the OpenCode global
configuration, which OpenCode reads from its own credentials store.

## 2. Requirements

- **OpenCode v2** (v2.0.27 was the version this configuration was built and
  verified against).
- A provider account. The default model is served by **OpenCode Go**.

```bash
opencode --version
opencode auth login            # sign in to the provider
opencode models                 # list the models your account can use
```

## 3. The default model, and how to override it

The committed default is:

```jsonc
"model": "opencode-go/minimax-m3.1-flash-free"
```

It was verified against OpenCode v2.0.27 to answer, to call tools and to
launch subagents. It is a **free tier** model, but free tiers have rate
limits and the catalogue changes; check `opencode models` before assuming it
is still available to your account.

### The override ladder

Use the first one that fits. **Never edit the committed `opencode.jsonc` to
change the model for your own machine** — that would force your choice on
every other contributor.

**1. Per-clone override (recommended).** Create `.opencode/opencode.jsonc`:

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "model": "opencode-go/glm-5.3-flash"
}
```

The file is git-ignored. `.opencode/opencode.jsonc` takes precedence over the
committed `opencode.jsonc` in the same directory — this was verified against
the installed CLI with `opencode debug config`.

**2. Machine-wide override.** In `~/.config/opencode/opencode.jsonc`:

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "model": "anthropic/claude-sonnet-4-5"
}
```

Note that the **global** file has the *lowest* precedence of the files
OpenCode merges: a project-level `model` wins over it. Use this when you want
a default for every project and do not use ATOM's.

**3. One-off.** For a single command, without touching any file:

```bash
opencode run --model opencode-go/qwen3.8-flash "add a search index to notes"
```

**4. In-session.** Use `/models` in the TUI. The selection applies to the
current session and does not rewrite any configuration file.

### Switching to a paid provider

If you want to use a metered provider, set it in your own override file (or
the global one) and make sure your account is signed in. The committed
configuration deliberately does **not** configure any provider endpoint or
credential, so it cannot spend your money by itself.

The committed configuration also does not deny any provider: choosing a
paid model is a deliberate act, and the agents are instructed to report it
rather than switch silently.

## 4. Permissions, and what they do and do not protect

The committed `opencode.jsonc` grants broad autonomy for local development and
asks before anything with an external consequence.

### Runs without a prompt

- Reading, searching and listing repository files.
- Editing project source, configuration and documentation.
- `cargo fmt`, `cargo clippy`, `cargo test`, `cargo build`, `cargo run`.
- `git status`, `git diff`, `git log`, `git show`.
- Any other non-destructive shell command.
- Launching subagents and asking you a question.
- Reading files outside the repository is **asked** first.

### Asks first

- `git push`, `git reset --hard`, `git clean`, `git checkout --`, `git rebase`,
  `git branch -D`.
- `cargo publish`, `npm publish`, `docker push`, `gh release create`.
- `kubectl apply`, `kubectl delete`, `terraform apply`.
- `psql` of any kind.
- Reading a real `.env` file (the committed `.env.example` stays readable).

### Refused outright

- `git push --force`, `git push --delete`, `git remote remove`.
- `git clean -fdx`, `rm -rf ~`, `rm -rf /*`.
- `gh repo delete`.
- Reading anything under `.ssh/`, or `*.pem`, `*.key`, `id_rsa*`,
  `credentials.json`, `secrets/`.

### What permissions do *not* do

- They do not remove operating-system restrictions. A command the OS refuses
  is still refused.
- They do not remove provider rate limits or account quotas.
- They do not stop a tool from being slow or wrong.
- They do not review your work. The `code-reviewer` and `quality-gate` agents
  do that.

## 5. The agent team

| Agent | Mode | Owns |
| ----- | ---- | ---- |
| `orchestrator` | primary | Planning, delegation, integration, verification. The only primary agent. |
| `architect` | subagent | Design, module boundaries, contracts, ADRs. |
| `rust-backend` | subagent | Handlers, services, DTOs, the Axum wiring. |
| `auth-security` | subagent | Authentication, tokens, authorization, secret handling, security tests. |
| `database` | subagent | Schema, migrations, constraints, indexes, query safety. |
| `test-engineer` | subagent | Unit, integration, contract and security tests. |
| `code-reviewer` | subagent | Read-only review of a diff. |
| `quality-gate` | subagent | Read-only verification. Returns `PASS` or `FAIL`. |
| `docs` | subagent | The knowledge base under `docs/`. |
| `devops` | subagent | CI, build, containers, dependencies, release plumbing. |

The two read-only agents cannot edit files: their permissions deny `edit`
outright, restrict `shell` to an allow-list of inspection commands, and deny
the `execute` action, which would otherwise give them Code Mode and with it
the tools that write files. This was verified behaviourally: an attempt by
the `code-reviewer` to create a file was refused with `Permission denied:
shell`, and the file was not created.

### Delegation, and how deep it goes

The orchestrator launches the specialists with the `subagent` tool. The
committed configuration sets `experimental.subagent_depth: 2`, which allows a
specialist to launch one nested helper of its own. This was verified against
the installed CLI: a subagent holding a `subagent` allow rule successfully
launched a second subagent and returned its result.

The platform default is a depth of 1, which forbids nesting. If a future
OpenCode release removes the experimental flag, the team still works; it
simply cannot nest, and the `atom-specialist` skill falls back to having the
orchestrator launch the extra agent itself.

### Adding a specialist

When a task needs a lens the standing team does not cover, the
`atom-specialist` skill explains how to add one for the duration of the task.
Prefer a temporary specialist over stretching an existing role.

## 6. Commands and skills

| Command | What it does |
| ------- | ------------ |
| `/feature <name>` | Runs the full delivery workflow for a feature. |
| `/review` | Asks the read-only `code-reviewer` to inspect the current changes. |
| `/verify` | Asks the read-only `quality-gate` to judge the evidence. |

| Skill | What it does |
| ----- | ------------ |
| `atom-delivery` | The end-to-end workflow for a substantial change. |
| `atom-docs` | The mandatory documentation intake and update protocol. |
| `atom-specialist` | How to add a specialist agent on demand. |

## 7. Validating the configuration

```bash
./scripts/validate-opencode.sh          # Unix
pwsh ./scripts/validate-opencode.ps1   # Windows PowerShell
```

The script checks the OpenCode version, lists the configuration sources,
lists the discovered agents, verifies that the configured model is in your
catalogue, parses the configuration files and scans them for
credential-shaped values.

The documentation tree has its own checker:

```bash
cargo run --bin atom-docs
```

## 8. Version notes — verified behaviour of OpenCode v2.0.27

These are the non-obvious details that were established by testing the
installed CLI, not by reading the schema. They matter when you edit the
configuration.

### The enforced policy key is `experimental.policies`

The current schema documents a top-level `policies` array. **OpenCode v2.0.27
ignores it.** A policy placed at the top level had no effect in testing; the
same policy placed under `experimental.policies` blocked the command with a
`null` output and the message "Blocked by configuration policy". This
repository therefore keeps its policies under `experimental.policies`.

If you upgrade OpenCode and the top-level key starts working, move them
there and re-run the validation script.

### Agent colours must be quoted

In an agent's YAML frontmatter, write:

```yaml
color: "#3b82f6"
```

An **unquoted** `color: #3b82f6` is read by the YAML parser as a value
followed by a comment, so the colour becomes empty, the agent fails
validation, and OpenCode **silently drops the agent**. The agent then cannot
be launched and does not appear in `opencode debug agents`.

### The agent description is required

An agent without a `description` is dropped. Keep the description to one
sentence: OpenCode shows it to the model when it chooses which agent to
launch.

### Primary and subagent files

- `.opencode/agents/<name>.md` — a subagent.
- `.opencode/modes/<name>.md` — a primary agent.

A file in `agents/` may still declare `mode: primary` in its frontmatter;
that is how `orchestrator` is defined here.

### The agent id is the file path

The file name becomes the agent id. A nested path becomes
`team/reviewer`. Nested directories under `agents/` are supported.

## 9. Troubleshooting

| Symptom | Cause | Fix |
| ------- | ----- | --- |
| `Agent not found: "x"` | The file failed validation — most often an unquoted `color`, a missing `description`, or invalid YAML. | Run `opencode debug agents` and check that `x` is listed. |
| The agent is listed but is missing its permissions | The `permissions:` block was dropped. | Keep the block directly under the frontmatter, one rule per three lines. |
| A command that should be blocked is not | The policy is at the top level instead of under `experimental.policies`. | Move it, then re-test. |
| `model` in the global config is ignored | The project configuration sets `model`, and the project file wins. | Use `.opencode/opencode.jsonc` instead. |
| The configured model is not offered | Your account does not have that provider or model. | Run `opencode models`; pick another and put it in your local override. |
| The model answers but never calls a tool | The model has no tool support. | Choose a different model; the catalogue marks tool-capable models. |
| Delegation returns "permission denied" | The parent agent's `subagent` permission denied it. | Check the parent agent's `permissions` block. |
| Rate limited immediately | The authentication rate limit is per client address. | Wait for the window to reset; the response carries `retry-after`. |
| A subagent cannot ask you a question | Subagents deny the `question` action by design; they report back to the orchestrator instead. | Let the orchestrator ask. |
| The project configuration is ignored | `OPENCODE_DISABLE_PROJECT_CONFIG=1` is set, or OpenCode was started outside the repository. | Start OpenCode in the repository root. |
| `opencode debug config` shows your global file last | Configuration precedence: system → global → project `opencode.jsonc` → project `.opencode/opencode.jsonc`. | Use the higher-precedence file. |

## 10. Checking what the agents actually did

- `opencode debug config` — every configuration source, in precedence order.
- `opencode debug agents` — every discovered agent with its resolved
  permissions.
- `opencode debug paths` — the data, config, cache and state directories.
- `opencode service status` — the shared background service.
- `~/.local/share/opencode/log/opencode.log` — the server log. It records
  configuration reloads, permission decisions and tool activity. **Remove
  secrets before sharing a log excerpt.**
