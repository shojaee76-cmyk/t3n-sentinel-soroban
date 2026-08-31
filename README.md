# t3n-sentinel-soroban

**Private API-key vault & health sentinel for AI agents — Soroban port.**

This is the third instantiation of the `t3n-sentinel` architecture:

| Port | Platform | Status |
|---|---|---|
| [t3n-sentinel](https://github.com/shojaee76-cmyk/t3n-sentinel) | T3N TEE (WASM) | live on testnet, contract id 741, 3 providers probed VALID |
| [t3n-sentinel-solana](https://github.com/shojaee76-cmyk/t3n-sentinel-solana) | Solana (Anchor) | M1+M2 code-complete, 20/20 tests green |
| **t3n-sentinel-soroban** (this repo) | Stellar Soroban | **51/51 tests green** |

The shape of the API is identical across all ports
(`init / seal / probe / list / rotate / history`). The storage model moves
from a host-bound KV map to the Soroban ledger.

## Contracts

| Contract | Path | Purpose | Tests |
|---|---|---|---|
| `sentinel-vault` | `contracts/sentinel-vault/` | ACL'd secret vault + audit-trail ring buffer | 18 |
| `sentinel-oracle` | `contracts/sentinel-oracle/` | TEE attestation oracle; emits `ProbeFired` only for valid attestations | 11 |
| `sentinel-payment` | `contracts/sentinel-payment/` | **Atomic XLM micropayment rail** — per-probe payment to provider payout | 11 |
| `sentinel-sac` | `contracts/sentinel-sac/` | **SAC-denominated rail** — audit trail in USDC-on-Stellar (or any SAC) | 11 |

## Build & test

```bash
cargo test          # 51/51 green (native, against the Soroban SDK testutils)
```

## Security model

1. **Key material is stored per (vault, provider)** under the `SECRETS` map.
   The actual key material is held by a TEE worker registered in the contract;
   the contract holds the access policy and the audit log.
2. **A `tee_worker` address is the ONLY caller authorized to invoke
   `record_probe`** (the off-chain TEE adapter that does the HTTP probe).
3. **`HISTORY` is an append-only ring buffer** (16 entries).
4. **The probe function NEVER returns the API key** — only the verdict
   (`VALID | INVALID | RATE_LIMITED | UNEXPECTED`), matching the T3N egress
   shape exactly.
5. **`sentinel-oracle` gates probe verdicts on a TEE attestation** — the
   off-chain verifier (Phala / Nillion / SGX/TDX quote verifier) checks the
   real quote and, on success, submits the validated attestation digest here.
   The oracle enforces per-epoch attestations with a replay guard.
6. **`sentinel-payment` / `sentinel-sac`**: payment is ATOMIC with the probe —
   the transfer happens BEFORE the receipt is appended, so the invariant
   "no probe without payment when provider is paywalled" holds by construction.
   The contract holds the token balance; the TEE worker never holds funds.

## Maintenance contract

Adding a new provider = appending ONE entry to `PROVIDERS`
(`contracts/sentinel-vault/src/providers.rs`). No schema migration, no client
update. (Same as the T3N + Solana ports.)

## Roadmap

- [x] M1: `sentinel_vault.rs` + `sentinel_oracle.rs` (29/29 tests green)
- [x] M2: `sentinel_payment.rs` — atomic XLM micropayment rail (11/11)
- [x] M3: `sentinel_sac.rs` — Stellar Asset Contract (USDC-on-Stellar) integration (11/11)
- [ ] Deploy on Soroban testnet (see `scripts/deploy_testnet.sh`)
- [ ] End-to-end narrated demo

## License

MIT
