# `common`

**Purpose.** Shared library used by both sample servers so auth and dummy helpers are not duplicated.

**What it does.** Provides AuthService implementation, token store, leader-schedule allowlist, and helpers to log P2C batches / build dummy tip bundles / warn on bad public URLs.

| Module | Purpose | What it provides |
|--------|---------|------------------|
| `auth` | Bearer auth for Relayer / Validator | `AuthServiceImpl`, `require_bearer` |
| `tokens` | Issue and store access / refresh tokens | `TokenStore`, `default_ttls` |
| `leader_schedule` | Decide which pubkeys may connect | Allowlist: leader schedule ∩ Rakurai client id |
| `sample` | Demo helpers for the two binaries | `log_p2c_batch`, `make_dummy_bundle`, `warn_if_bad_public_url` |
