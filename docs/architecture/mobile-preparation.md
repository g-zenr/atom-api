# Mobile application preparation (React Native + Expo)

> **Status:** Preparation document — nothing here is implemented; the mobile
> client remains excluded by [`../project-truth.md`](../project-truth.md) §6.2
> until its own amendment accepts it
> **Last updated:** 2026-10-30
> **Owner:** maintained by the `architect` agent

This document records the decisions a future mobile application can start
from, so that the backend and the web client are not shaped by a mobile
design that arrives later. It is preparation, not a delivery record: no Expo
project exists, and none may be built until the owner accepts the mobile
frontend as scope (the same amendment mechanism that accepted the web client
on 2026-10-30).

---

## 1. What the mobile client consumes

The same contract the web client consumes:

- **The Rust API is the only backend.** `/api/v1` bearer authentication, the
  one error envelope, the pagination envelope, and `ADR-0007` money strings
  apply unchanged. The generated OpenAPI contract
  (`/api/v1/openapi.json`) is the shared source; the web client's generated
  types (`atom-web`) are produced from it, and the mobile client generates
  its own from the same document rather than copying the web package.
- **No direct database access, ever.** PostgreSQL is behind the API for every
  client.

## 2. Authentication and token storage

- The bearer-token model suits mobile without the web's cookie/CSRF
  obligations (the roadmap already notes this). Login and refresh use the
  existing `/auth/login` and `/auth/refresh` endpoints with no backend change.
- **Secure storage, not localStorage:** the access token lives in memory and
  the refresh token in the platform secure store (iOS Keychain / Android
  Keystore, e.g. `expo-secure-store`). The refresh flow is the existing
  rotation with reuse detection — a stolen refresh token from a compromised
  device is detectable and revocable exactly as designed.
- Session expiry handling: on `401` from any call, attempt one refresh; on
  failure, return the user to login. Logout calls `/auth/logout` and clears
  the secure store.
- The open question `Q-08` (a password-change and session-management surface)
  is the backend gap a mobile settings screen would meet first.

## 3. Suggested structure (Expo Router)

A minimal route tree, mirroring the web MVP order so the two clients stay
comparable:

```text
app/
├── _layout.tsx          # providers: query client, auth session
├── (auth)/
│   ├── login.tsx
│   └── register.tsx
├── (tabs)/
│   ├── dashboard.tsx    # reads existing list endpoints; no invented summaries
│   ├── notes.tsx        # list / search / create / edit / archive
│   ├── projects.tsx     # with tasks inside a project detail
│   └── settings.tsx     # sessions, logout, (future) password change
```

State management mirrors the web approach: a small typed API client generated
from `openapi.json`, a query library for caching, and no business logic that
the backend does not already own.

## 4. Offline behaviour

Phase 1 stance, recorded so it is a decision rather than an accident: the
first mobile release is **online-first**. Read caches may serve stale content
while a request is in flight, but there is no offline write queue, no conflict
resolution, and no local persistence beyond the token store. An offline write
queue would need an idempotency design on the backend first (the pattern
sketched in [`ADR-0012`](../decisions/ADR-0012-automation-orchestration-scope.md)
would be the precedent) — that is a separate decision.

## 5. Push notifications — prerequisites

Push is not built and needs all three of:

1. A backend record of *what* to notify about (the notification feature is not
   built; the roadmap's "Notifications and approval workflows" row is
   `Proposed`).
2. A delivery channel decision (Expo push via its hosted service is the
   low-cost default, but it is a **hosted dependency** —
   `project-truth.md` §8 forbids hosted services *for the build*; a
   push relay is a runtime product dependency the owner would have to accept
   explicitly, like the n8n deployment in [`ADR-0012`](../decisions/ADR-0012-automation-orchestration-scope.md)).
3. Device-token storage with owner scoping (a new table, following the
   ownership conventions).

## 6. Development and release configuration

- Expo EAS is the intended build/release path, but it is also a hosted
  service; the same owner decision as §5.2 applies. A fully local
  `npx expo run:[ios|android]` workflow works without any hosted dependency
  for development.
- API base URL per environment (`development` / `production`) with no secrets
  in the bundle — the same rule the web client follows.
- TLS is mandatory on any non-loopback base URL; the API's own deployment
  terminates plain HTTP behind a reverse proxy today
  ([`deployment.md`](deployment.md)).

## 7. What must be true before mobile work starts

1. The web MVP is stable (the shared contract is exercised by one real
   consumer).
2. The owner accepts the mobile frontend (a §6.2-style amendment).
3. `Q-08` (password change / session management) has an answer, or the
   settings screen ships without it.
4. The push-notification owner decision (§5) is made, or push is explicitly
   out of the first release.

Nothing in this document blocks backend or web work; it exists so the backend
does not need reshaping when the answer to any of the four arrives.
