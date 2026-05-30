# srvcs-exp

The natural-exponential primitive of the srvcs.cloud distributed standard
library.

Its single concern: **compute `e^value`** (the natural exponential). It does not
validate input itself — it delegates "is this a number" to
[`srvcs-isnumber`](https://github.com/srvcs/isnumber) over HTTP, the single
source of truth for that question.

`srvcs-exp` is a **floating-point** service: it accepts both integers and floats
as input and returns a floating-point `result` (an `f64`). For example,
`exp(0) == 1.0` and `exp(1) ~= 2.718281828459045`.

If `srvcs-isnumber` is unreachable, `srvcs-exp` reports itself **degraded
(503)** rather than guessing.

## API

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/` | Service identity, concern, and dependency list |
| `POST` | `/` | Compute `exp(value)` |
| `GET` | `/healthz` `/readyz` `/metrics` `/openapi.json` | srvcs service standard surface |

```sh
curl -s -X POST localhost:8080/ -H 'content-type: application/json' -d '{"value": 1}'
# {"value":1,"result":2.718281828459045}
```

Responses:

- `200 {"value": n, "result": float}` — evaluated.
- `422` — the value is not a number (per `srvcs-isnumber`).
- `503` — a dependency is unavailable.

## Dependencies

- [`srvcs-isnumber`](https://github.com/srvcs/isnumber) — input validation.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `SRVCS_BIND_ADDR` | `0.0.0.0:8080` | Bind address |
| `SRVCS_ISNUMBER_URL` | `http://127.0.0.1:8081` | Base URL of `srvcs-isnumber` |
| `SRVCS_ENV` | `development` | Environment label for logs |
| `RUST_LOG` | `info,tower_http=info` | Tracing filter |

## Local checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Orchestration tests stand up a mock `srvcs-isnumber` in-process (one that
genuinely computes "is this a number" from the request body), so the suite runs
without the rest of the fleet. Float results are compared approximately
(`|got - expected| < 1e-9`), never with exact equality. See
[`srvcs/platform`](https://github.com/srvcs/platform) for the shared standard.

> Note: the `cargoHash` in `flake.nix` is inherited from the template and must be
> refreshed with a `nix build` before the Nix gates pass.
