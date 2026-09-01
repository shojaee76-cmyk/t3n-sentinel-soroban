# t3n-sentinel-soroban

**Private API-key vault & health sentinel for AI agents — Soroban port.**

This is the third instantiation of the `t3n-sentinel` architecture:

| Port | Platform | Status |
|---|---|---|
| [t3n-sentinel](https://github.com/shojaee76-cmyk/t3n-sentinel) | T3N TEE (WASM) | live on testnet, contract id 741, 3 providers probed VALID |
| [t3n-sentinel-solana](https://github.com/shojaee76-cmyk/t3n-sentinel-solana) | Solana (Anchor) | M1+M2 code-complete, 20/20 tests green |
| **t3n-sentinel-soroban** (this repo) | Stellar Soroban | **LIVE on testnet — all 4 contracts deployed & verified** |

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

## Live deployment (Soroban testnet)

Deployed and verified 2026-09-01 (Testnet, protocol 28). Account: `sentinel`
(`GDFUVJ47JMNKUCPUJHITIZ6GFWEEQCZNNJHJOWGRPLXE3OS6NIZTLUWM`, funded via friendbot).

| Contract | Address | Explorer |
|---|---|---|
| `sentinel-vault` | `CBK4B7267LVCI2C3ZY66DAYRNGCNXGDGP6DDV2SRSHMZ5ZK7GRC2VKXF` | [lab.stellar.org](https://lab.stellar.org/r/testnet/contract/CBK4B7267LVCI2C3ZY66DAYRNGCNXGDGP6DDV2SRSHMZ5ZK7GRC2VKXF) |
| `sentinel-oracle` | `CC4L4EB7BXJXKRFO6CGNWOHIT4JOXEHE66YKSOYPS2XXK4RSFRX37LIP` | [lab.stellar.org](https://lab.stellar.org/r/testnet/contract/CC4L4EB7BXJXKRFO6CGNWOHIT4JOXEHE66YKSOYPS2XXK4RSFRX37LIP) |
| `sentinel-payment` | `CDKZ5KQCSYELCE6QFQ2IVNHFMZG7QIF6Q54RTRJGVVGIRPIEANSAZQAU` | [lab.stellar.org](https://lab.stellar.org/r/testnet/contract/CDKZ5KQCSYELCE6QFQ2IVNHFMZG7QIF6Q54RTRJGVVGIRPIEANSAZQAU) |
| `sentinel-sac` | `CATNAPASG4ZZ3MVJ5Q52O5FYHOACUPDFYTBZKDPTHDVSPE2J6RD7ORAH` | [lab.stellar.org](https://lab.stellar.org/r/testnet/contract/CATNAPASG4ZZ3MVJ5Q52O5FYHOACUPDFYTBZKDPTHDVSPE2J6RD7ORAH) |
| USDC SAC (demo asset) | `CBEDI6AAA7AK2CB6SVLZSNMVDTRRZR6D5T22UQUZFD6MQKSCZ7GOAGYQ` | — |

Native XLM SAC: `CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC`.

**On-chain lifecycle verified** (real txs, all `VALID`):
- vault: `seal` → `record_probe(200)` → `list_providers` shows `github: VALID` → `history` has the receipt
- oracle: `submit_attestation(phala)` → `probe` → `is_verified(github) == true`, epoch 0
- payment: `configure_provider(price=100, paywalled)` → fund 1000 XLM → `probe_with_payment(100)` → transfer event 100 XLM to payout, balance 1000→900, receipt `paid: 100`
- sac: mint 5000 USDC → `probe_with_payment(50)` → burn event 50 USDC, balance 5000→4950, receipt `paid: 50`

Deploy tooling: `stellar-cli` 27.1.0 (`stellar.exe` in `C:/Users/capit/stellar-cli/`).

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
- [x] **Deploy on Soroban testnet (all 4 contracts, on-chain verified)**
- [x] **End-to-end narrated demo** — `docs/scf-demo/t3n-sentinel-scf46-demo.mp4`
  (4:13, 1080p, real on-chain tx hashes baked in — XLM 1000→900,
  USDC 5000→4950. Build: `python docs/scf-demo/build_scf_demo.py`).
  **Watch online:** https://drive.google.com/file/d/1ElNeJXEzPJBJwr900B5GOhGx41YZ9DNF/view

## License

MIT
