#!/usr/bin/env python3
"""
build_scf_demo.py — produce the t3n-sentinel Stellar Community Fund #46 demo video.

Renders 10 slide cards (1920x1080) via matplotlib, generates narration
via morteza_tts (Edge TTS, en-US-ChristopherNeural), and assembles a
single 1080p MP4 with ffmpeg. Same dark-terminal aesthetic as the x402
and Circle demo videos.

Content (per the SCF #46 Build round — 150k XLM, deadline Nov 8 2026):
  (a) t3n-sentinel architecture — vault → TEE oracle → verdict
  (b) the LIVE Soroban contracts — real addresses + stellar.expert links
  (c) the paid-probe rail — real XLM transfer + USDC burn, real tx hashes
  (d) 4-chain matrix evidence — 119 tests, T3N live id 741, Sepolia + testnet live
  (e) roadmap mapped to SCF milestones — M1/M2/M3 all DONE on-chain

Usage:
  python docs/scf-demo/build_scf_demo.py
  # outputs docs/scf-demo/t3n-sentinel-scf46-demo.mp4
"""
import json
import os
import subprocess
import sys
import time
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import FancyBboxPatch, Rectangle

# ---------------------------------------------------------------------------
# Live data (real on-chain values, verified 2026-09-01)
# ---------------------------------------------------------------------------
LIVE = {
    "vault":    "CBK4B7267LVCI2C3ZY66DAYRNGCNXGDGP6DDV2SRSHMZ5ZK7GRC2VKXF",
    "oracle":   "CC4L4EB7BXJXKRFO6CGNWOHIT4JOXEHE66YKSOYPS2XXK4RSFRX37LIP",
    "payment":  "CDKZ5KQCSYELCE6QFQ2IVNHFMZG7QIF6Q54RTRJGVVGIRPIEANSAZQAU",
    "sac":      "CATNAPASG4ZZ3MVJ5Q52O5FYHOACUPDFYTBZKDPTHDVSPE2J6RD7ORAH",
    "usdc_sac": "CBEDI6AAA7AK2CB6SVLZSNMVDTRRZR6D5T22UQUZFD6MQKSCZ7GOAGYQ",
    "xlm_sac":  "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
    "identity": "GDFUVJ47JMNKUCPUJHITIZ6GFWEEQCZNNJHJOWGRPLXE3OS6NIZTLUWM",
    "xlm_fund_tx":   "0x85178e7a8c5ec5868b27...",  # create_account + fund
    "xlm_probe_tx":  "bb71662d93ccc138a3f5c5717287d972be3e785ee2548d76f6c2716243596b1f",
    "xlm_probe_tx2": "d91b3d03cce8e87db6178ce3b5c5663abb25fe99a8210a4658c1c0037b5f0600",
    "xlm_probe_ledger": 4448159,
    "usdc_probe_tx": "63316632344850dd1df9cc72bd94efd6e4346efca63f0385f58f5be55df0b812",
    "usdc_mint_tx": "ed9933cbf16e8b8cbd6e0268401bace1362c8df3b4facf80dd38f6b67cc5e4d3",
    "usdc_probe_ledger": 4448189,
    "t3n_contract_id": "741",
    "t3n_providers":   "3/3 VALID (GitHub, Groq, OpenRouter)",
    "tests_total": 119,
    "repos": [
        ("t3n-sentinel",         "T3N TEE (WASM)",        "LIVE · id 741 · 5/5"),
        ("t3n-sentinel-solana",  "Solana (Anchor)",       "20/20"),
        ("t3n-sentinel-soroban", "Stellar (Soroban)",     "LIVE testnet · 51/51"),
        ("t3n-sentinel-starknet","Starknet (Cairo)",      "LIVE Sepolia · 43/43"),
    ],
    "github_base": "https://github.com/shojaee76-cmyk/",
}

# ---------------------------------------------------------------------------
# Theme (dark terminal aesthetic, Stellar purple accent)
# ---------------------------------------------------------------------------
BG    = "#0B0F14"
PANEL = "#121820"
LINE  = "#1E2833"
FG    = "#E6EDF3"
MUT   = "#8B98A5"
ACC   = "#34D399"   # green — money/verdict
ACC2  = "#60A5FA"   # blue — links/chain
WARN  = "#F87171"   # red — paywall
STEL  = "#9C6BFF"   # Stellar purple — brand accent
MONO  = "Consolas"
SANS  = "DejaVu Sans"

OUT = Path(r"C:\Users\capit\t3n-sentinel-soroban\docs\scf-demo")
SLIDES = OUT / "slides"
NARR = OUT / "narration"
SEGS = OUT / "segments"
for d in (SLIDES, NARR, SEGS):
    d.mkdir(parents=True, exist_ok=True)


def slide(name, draw):
    fig = plt.figure(figsize=(19.2, 10.8), dpi=100)
    fig.patch.set_facecolor(BG)
    ax = fig.add_axes([0, 0, 1, 1])
    ax.set_xlim(0, 1920)
    ax.set_ylim(0, 1080)
    ax.axis("off")
    draw(ax)
    fig.savefig(SLIDES / f"{name}.png", facecolor=BG)
    plt.close(fig)
    print(f"  slide {name}")


def box(ax, x, y, w, h, fc=PANEL, ec=LINE, lw=2, r=16):
    ax.add_patch(FancyBboxPatch((x, y), w, h,
                 boxstyle=f"round,pad=0,rounding_size={r}",
                 fc=fc, ec=ec, lw=lw))


def T(ax, x, y, s, size=28, color=FG, weight="normal",
      ha="left", va="center", family=MONO, ls=1.4):
    ax.text(x, y, s, fontsize=size, color=color, fontweight=weight,
            ha=ha, va=va, family=family, linespacing=ls)


def chrome(ax, title, step):
    T(ax, 60, 1020, "T3N-SENTINEL  ·  STELLAR COMMUNITY FUND  #46", 20, MUT, "bold")
    T(ax, 1860, 1020, step, 20, STEL, "bold", ha="right")
    ax.plot([60, 1860], [985, 985], color=LINE, lw=1.5)
    T(ax, 60, 60, title, 34, FG, "bold", family=SANS)
    ax.plot([60, 1860], [95, 95], color=LINE, lw=1.5)


def wrap(s, w):
    import textwrap
    return "\n".join(textwrap.wrap(s, w))


# ---------------------------------------------------------------------------
# Slides
# ---------------------------------------------------------------------------
def s01_hook(ax):
    """Title — 0:00-0:10."""
    chrome(ax, "", "1 / 10")
    T(ax, 960, 640, "t3n-sentinel", 110, FG, "bold", ha="center", family=SANS)
    T(ax, 960, 520, "An agentic-compute security rail — on Stellar.",
      40, MUT, ha="center", family=SANS)
    box(ax, 360, 330, 1200, 90, fc="#0F1520", ec=STEL, lw=2.5)
    T(ax, 960, 375, "Stellar Community Fund  #46  ·  Build round  ·  up to 150k XLM",
      28, STEL, "bold", ha="center", family=MONO)
    T(ax, 960, 220, "TEE-gated API-key vault  ·  atomic XLM / USDC probe payments  ·  4 chains",
      24, MUT, ha="center", family=SANS)


def s02_architecture(ax):
    """(a) t3n-sentinel architecture — vault → TEE oracle → verdict."""
    chrome(ax, "The architecture — vault, TEE oracle, verdict", "2 / 10")
    box(ax, 120, 400, 480, 300, fc=PANEL, ec=ACC2, lw=2.5)
    T(ax, 360, 640, "SENTINEL VAULT", 26, ACC2, "bold", ha="center")
    T(ax, 360, 580, "ACL'd key store", 22, FG, ha="center")
    T(ax, 360, 540, "per-provider secrets", 20, MUT, ha="center")
    T(ax, 360, 500, "16-entry ring buffer", 20, MUT, ha="center")

    box(ax, 720, 400, 480, 300, fc=PANEL, ec=STEL, lw=2.5)
    T(ax, 960, 640, "TEE ORACLE", 26, STEL, "bold", ha="center")
    T(ax, 960, 580, "attestation-gated", 22, FG, ha="center")
    T(ax, 960, 540, "replay-guarded", 20, MUT, ha="center")
    T(ax, 960, 500, "per-epoch", 20, MUT, ha="center")

    box(ax, 1320, 400, 480, 300, fc=PANEL, ec=ACC, lw=2.5)
    T(ax, 1560, 640, "VERDICT", 26, ACC, "bold", ha="center")
    T(ax, 1560, 580, "VALID / INVALID", 22, FG, ha="center")
    T(ax, 1560, 540, "RATE_LIMITED", 20, MUT, ha="center")
    T(ax, 1560, 500, "UNEXPECTED", 20, MUT, ha="center")

    for x1, x2 in [(600, 720), (1200, 1320)]:
        ax.annotate("", xy=(x2, 550), xytext=(x1, 550),
                    arrowprops=dict(arrowstyle="-|>", color=MUT, lw=3))
    T(ax, 960, 250, "The probe function NEVER returns the key — only the verdict.",
      26, FG, "bold", ha="center", family=SANS)
    T(ax, 960, 200, "Same API shape on T3N WASM · Solana · Stellar · Starknet",
      22, MUT, ha="center", family=SANS)


def s03_live_contracts(ax):
    """(b) LIVE Soroban contracts — real addresses + stellar.expert links."""
    chrome(ax, "Live on Soroban testnet — protocol 28, real addresses", "3 / 10")
    rows = [
        ("sentinel-vault",   LIVE["vault"],   "seal · probe · history"),
        ("sentinel-oracle",  LIVE["oracle"],  "attestation · replay-guard"),
        ("sentinel-payment", LIVE["payment"], "atomic XLM rail · 1000→900"),
        ("sentinel-sac",     LIVE["sac"],     "USDC rail · 5000→4950"),
    ]
    y = 820
    for name, addr, note in rows:
        box(ax, 140, y - 50, 1640, 100, fc=PANEL, ec=LINE, lw=1.5)
        T(ax, 180, y, name, 26, STEL, "bold", family=MONO)
        T(ax, 620, y, addr[:12] + "…" + addr[-6:], 22, FG, family=MONO)
        T(ax, 1720, y, note, 19, MUT, family=SANS, ha="right")
        y -= 150
    T(ax, 960, 140, "Verified on-chain · deployer identity funded via friendbot · all txs public",
      22, MUT, ha="center", family=SANS)


def s04_xlm_rail(ax):
    """(c1) XLM paid-probe rail — real transfer event."""
    chrome(ax, "The paid-probe rail — real XLM transfer on-chain", "4 / 10")
    # left: rail flow
    box(ax, 120, 300, 720, 520, fc="#0F1520", ec=ACC, lw=2.5)
    T(ax, 480, 760, "XLM MICROPAYMENT RAIL", 26, ACC, "bold", ha="center")
    T(ax, 480, 690, "configure_provider  price=100 · paywalled", 20, MUT, ha="center")
    T(ax, 480, 640, "fund  →  1000 XLM", 24, FG, "bold", ha="center")
    T(ax, 480, 580, "probe_with_payment(100)", 22, FG, ha="center")
    T(ax, 480, 520, "→ transfer 100 XLM to payout", 22, ACC, "bold", ha="center")
    T(ax, 480, 460, "balance  1000 → 900", 24, FG, "bold", ha="center")
    T(ax, 480, 400, "receipt  paid: 100", 20, MUT, ha="center")
    T(ax, 480, 350, "ATOMIC: pay before probe receipt", 20, WARN, ha="center")

    # right: real tx hashes
    box(ax, 900, 300, 900, 520, fc=PANEL, ec=LINE, lw=2)
    T(ax, 940, 760, "REAL TRANSACTIONS (testnet)", 24, ACC, "bold")
    T(ax, 940, 690, "configure_provider", 20, MUT)
    T(ax, 940, 650, "0x73300fb1…", 20, FG, family=MONO)
    T(ax, 940, 590, "transfer  100 XLM", 22, FG, "bold", family=MONO)
    T(ax, 940, 550, LIVE["xlm_probe_tx"][:46], 16, ACC2, family=MONO)
    T(ax, 940, 515, "…" + LIVE["xlm_probe_tx"][-16:], 16, ACC2, family=MONO)
    T(ax, 940, 470, f"ledger {LIVE['xlm_probe_ledger']:,}", 20, MUT)
    T(ax, 940, 420, "probe_with_payment(100)", 22, FG, "bold", family=MONO)
    T(ax, 940, 380, LIVE["xlm_probe_tx2"][:46], 16, ACC2, family=MONO)
    T(ax, 940, 345, "…" + LIVE["xlm_probe_tx2"][-16:], 16, ACC2, family=MONO)
    T(ax, 940, 300, f"ledger {LIVE['xlm_probe_ledger']+4:,}", 20, MUT)


def s05_usdc_rail(ax):
    """(c2) USDC SAC rail — real burn event."""
    chrome(ax, "Same rail, USDC-on-Stellar — real burn event", "5 / 10")
    box(ax, 120, 300, 720, 520, fc="#0F1520", ec=ACC, lw=2.5)
    T(ax, 480, 760, "USDC STELLAR ASSET CONTRACT", 26, ACC, "bold", ha="center")
    T(ax, 480, 690, "mint   →  5000 USDC", 24, FG, "bold", ha="center")
    T(ax, 480, 630, "probe_with_payment(50)", 22, FG, ha="center")
    T(ax, 480, 570, "→ burn 50 USDC", 22, ACC, "bold", ha="center")
    T(ax, 480, 510, "balance  5000 → 4950", 24, FG, "bold", ha="center")
    T(ax, 480, 450, "receipt  paid: 50", 20, MUT, ha="center")
    T(ax, 480, 390, "any SAC works — USDC is the demo asset", 20, MUT, ha="center")
    T(ax, 480, 340, "classic asset: USDC:GDFUVJ…", 18, MUT, ha="center")

    box(ax, 900, 300, 900, 520, fc=PANEL, ec=LINE, lw=2)
    T(ax, 940, 760, "REAL TRANSACTIONS (testnet)", 24, ACC, "bold")
    T(ax, 940, 690, "mint 5000 (transfer)", 20, MUT)
    T(ax, 940, 650, LIVE["usdc_mint_tx"][:46], 16, ACC2, family=MONO)
    T(ax, 940, 615, "…" + LIVE["usdc_mint_tx"][-16:], 16, ACC2, family=MONO)
    T(ax, 940, 570, "probe_with_payment(50) + BURN", 22, FG, "bold", family=MONO)
    T(ax, 940, 530, LIVE["usdc_probe_tx"][:46], 16, ACC2, family=MONO)
    T(ax, 940, 495, "…" + LIVE["usdc_probe_tx"][-16:], 16, ACC2, family=MONO)
    T(ax, 940, 450, f"ledger {LIVE['usdc_probe_ledger']:,}  ·  protocol 28", 20, MUT)
    T(ax, 940, 390, "burn event on SAC:", 20, MUT)
    T(ax, 940, 350, "USDC:GDFUVJ47JMNKUCPUJHITIZ6GFWEE…", 18, FG, family=MONO)
    T(ax, 940, 310, "amount 50 · inSuccessfulCall", 18, ACC, family=MONO)


def s06_matrix(ax):
    """(d) 4-chain matrix evidence."""
    chrome(ax, "One architecture, four chains — all evidence public", "6 / 10")
    y = 780
    for name, platform, tests in LIVE["repos"]:
        box(ax, 260, y - 40, 1400, 90, fc=PANEL, ec=LINE, lw=1.5)
        T(ax, 300, y, name, 26, ACC2, "bold", family=MONO)
        T(ax, 1000, y, platform, 24, FG, family=SANS)
        T(ax, 1560, y, tests, 24, ACC, "bold", ha="right", family=MONO)
        y -= 130
    T(ax, 960, 200, f"{LIVE['tests_total']} contract tests passing across the fleet",
      30, FG, "bold", ha="center", family=SANS)
    T(ax, 960, 155, "All public · all MIT · all reproducible", 22, MUT, ha="center", family=SANS)
    T(ax, 960, 115, "github.com/shojaee76-cmyk", 22, ACC2, ha="center", family=MONO)


def s07_roadmap(ax):
    """(e) Roadmap mapped to SCF milestones — all done."""
    chrome(ax, "Roadmap → SCF milestones — every milestone is DONE", "7 / 10")
    m1 = ("SCF M1  ·  Vault + Oracle", "seal · probe · attestation", "LIVE on testnet, verified")
    m2 = ("SCF M2  ·  Payment rail", "atomic XLM micropayments", "LIVE — 1000→900 on-chain")
    m3 = ("SCF M3  ·  SAC / USDC", "any Stellar Asset Contract", "LIVE — 5000→4950 on-chain")
    x = 160
    for title, head, sub in (m1, m2, m3):
        box(ax, x, 560, 480, 280, fc=PANEL, ec=STEL if "M1" in title else LINE, lw=2.5)
        T(ax, x + 40, 780, title, 24, STEL, "bold", family=MONO)
        T(ax, x + 40, 720, head, 24, FG, "bold", family=SANS)
        T(ax, x + 40, 660, sub, 20, ACC, family=SANS)
        T(ax, x + 40, 610, "✓ done", 22, ACC, "bold", family=MONO)
        x += 520

    box(ax, 160, 240, 1600, 200, fc="#0F1520", ec=ACC, lw=2)
    T(ax, 960, 370, "The grant builds ON what is already live — not a promise.",
      28, FG, "bold", ha="center", family=SANS)
    T(ax, 960, 310, "SCF funds: mainnet readiness, audit, 10-provider matrix, public handoff",
      22, MUT, ha="center", family=SANS)
    T(ax, 960, 260, "Ask: up to 150,000 XLM  ·  build deadline Nov 8 2026",
      24, STEL, "bold", ha="center", family=MONO)


def s08_security(ax):
    """Security model — the invariant that matters."""
    chrome(ax, "Security model — payment is atomic with the probe", "8 / 10")
    box(ax, 200, 620, 1520, 150, fc="#0F1520", ec=WARN, lw=2)
    T(ax, 240, 700, "NO PROBE WITHOUT PAYMENT", 30, WARN, "bold")
    T(ax, 240, 645, "the transfer happens BEFORE the receipt is appended — invariant by construction",
      22, MUT)

    box(ax, 200, 420, 1520, 140, fc="#0F1520", ec=ACC2, lw=2)
    T(ax, 240, 500, "THE TEE WORKER NEVER HOLDS FUNDS", 30, ACC2, "bold")
    T(ax, 240, 445, "the contract holds the token balance; the worker only calls probe_with_payment",
      22, MUT)

    box(ax, 200, 220, 1520, 140, fc="#0F1520", ec=ACC, lw=2)
    T(ax, 240, 300, "KEYS NEVER LEAVE THE VAULT", 30, ACC, "bold")
    T(ax, 240, 245, "probe returns only the verdict — never the API key",
      22, MUT)

    T(ax, 960, 120, "Every invariant covered by tests: 51/51 green on Soroban alone",
      22, FG, ha="center", family=SANS)


def s09_traction(ax):
    """Traction — already shipped."""
    chrome(ax, "Traction — already shipped", "9 / 10")
    stats = [
        ("4", "chains ported"),
        ("119", "contract tests passing"),
        ("3", "live providers probed VALID"),
        ("2", "live testnet deployments"),
    ]
    x = 200
    for num, label in stats:
        box(ax, x, 450, 340, 260, fc=PANEL, ec=LINE, lw=2)
        T(ax, x + 170, 620, num, 64, ACC, "bold", ha="center", family=SANS)
        T(ax, x + 170, 540, label, 22, FG, ha="center", family=SANS)
        x += 380
    T(ax, 960, 280, "Live on T3N testnet (id 741) · Live on Starknet Sepolia ·",
      24, MUT, ha="center", family=SANS)
    T(ax, 960, 235, "Live on Soroban testnet — the full M1+M2+M3 stack, paid probes on-chain.",
      24, MUT, ha="center", family=SANS)


def s10_close(ax):
    """Close — ask."""
    chrome(ax, "", "10 / 10")
    T(ax, 960, 620, "t3n-sentinel: agents that pay their own way,", 52, FG, "bold", ha="center", family=SANS)
    T(ax, 960, 540, "atomically, on Stellar.", 52, STEL, "bold", ha="center", family=SANS)
    box(ax, 260, 340, 1400, 100, fc="#0F1520", ec=STEL, lw=2.5)
    T(ax, 960, 390, "github.com/shojaee76-cmyk/t3n-sentinel-soroban", 28, ACC2, "bold",
      ha="center", family=MONO)
    T(ax, 960, 220, "TEE-gated keys  ·  atomic XLM / USDC  ·  four chains  ·  one architecture",
      22, MUT, ha="center", family=SANS)


SLIDES_FN = [
    ("s01", s01_hook),
    ("s02", s02_architecture),
    ("s03", s03_live_contracts),
    ("s04", s04_xlm_rail),
    ("s05", s05_usdc_rail),
    ("s06", s06_matrix),
    ("s07", s07_roadmap),
    ("s08", s08_security),
    ("s09", s09_traction),
    ("s10", s10_close),
]

# ---------------------------------------------------------------------------
# Narration (~15 sec/slide → ~3 min total; target ≤ 5 min)
# ---------------------------------------------------------------------------
NARRATION = {
    "s01": "t3n-sentinel. An agentic-compute security rail, now live on Stellar. This is our Stellar Community Fund round forty-six application, in the build round, asking for up to one hundred fifty thousand X-L-M.",
    "s02": "The architecture is three pieces. The sentinel vault holds per-provider secrets behind an access-control list, with a sixteen-entry audit ring buffer. The T-E-E oracle gates every probe on a valid attestation, replay-guarded and per-epoch. The verdict classifies the provider: valid, invalid, rate-limited, or unexpected. The probe never returns the key — only the verdict.",
    "s03": "The entire stack is live on the Soroban testnet, protocol twenty-eight. Four contracts deployed and verified: the vault, the oracle, the payment rail, and the Stellar Asset Contract adapter. The deployer identity was funded via the testnet friendbot, and every transaction is public on the network.",
    "s04": "Here is the paid-probe rail with real events. We configured the provider paywalled at one hundred, funded the contract with one thousand X-L-M, then called probe with payment for one hundred. The transfer event is on-chain: one hundred X-L-M moved to the payout, the contract balance went from one thousand to nine hundred, and the receipt records paid one hundred. The transfer transaction hash is b-b-seven-one-six-two-d-d-nine-three; the probe transaction is d-nine-one-b-three-d-oh-three.",
    "s05": "The same rail works with any Stellar Asset Contract. We minted five thousand U-S-D-C, then probed for fifty. The burn event is on-chain: fifty U-S-D-C burned, the balance went from five thousand to forty-nine fifty, and the receipt records paid fifty. The probe transaction is six-three-three-one-six-six-three-two. One rail, any asset.",
    "s06": "The evidence is public across four chains. The original T3-N T-E-E deployment, live, contract id seven-four-one. The Solana Anchor port, twenty of twenty tests. The Stellar Soroban port, fifty-one of fifty-one, live on testnet. The Starknet Cairo port, forty-three of forty-three, live on Sepolia. One hundred nineteen contract tests passing across the fleet. All public, all M-I-T, all reproducible.",
    "s07": "Every S-C-F milestone is already done on-chain. Milestone one, the vault and oracle, live and verified. Milestone two, the atomic X-L-M payment rail, live — the one thousand to nine hundred transfer is on-chain. Milestone three, the S-A-C and U-S-D-C integration, live — the burn is on-chain. This grant builds on what is already live, not on a promise.",
    "s08": "The security model is the point. No probe without payment — the transfer happens before the receipt is appended, so the invariant holds by construction. The T-E-E worker never holds funds — the contract owns the balance. And keys never leave the vault — the probe returns only a verdict. Every invariant is covered by the fifty-one green tests on Soroban alone.",
    "s09": "Traction today: four chains ported, one hundred nineteen contract tests passing, three live providers probed valid, and two live testnet deployments — the T3-N T-E-E and the Starknet Sepolia stack, plus the full Soroban stack with paid probes on-chain.",
    "s10": "t3n-sentinel: agents that pay their own way, atomically, on Stellar. The code is public, the tests pass, the payments are on-chain. Thank you.",
}

# ---------------------------------------------------------------------------
# Run pipeline
# ---------------------------------------------------------------------------
def run(cmd, **kw):
    r = subprocess.run(cmd, capture_output=True, text=True, **kw)
    if r.returncode != 0:
        sys.stderr.write("CMD FAILED: " + " ".join(cmd)[:300] + "\n")
        sys.stderr.write(r.stderr[-2000:] + "\n")
        raise SystemExit(1)
    return r


def main():
    print("=== rendering slides ===")
    for name, fn in SLIDES_FN:
        slide(name, fn)

    print("\n=== generating narration ===")
    tts = Path(r"C:\Users\capit\AppData\Local\hermes\scripts\morteza_tts.py")
    tts_py = Path(r"C:\Users\capit\bounty-lab\.venv\Scripts\python.exe")
    for name, text in NARRATION.items():
        text_file = NARR / f"{name}.txt"
        mp3 = NARR / f"{name}.mp3"
        text_file.write_text(text, encoding="utf-8")
        run([str(tts_py), str(tts), str(text_file), str(mp3)])
        print(f"  narration {name}: {text[:60]}…")

    print("\n=== assembling segments ===")
    def dur(p):
        r = subprocess.run(["ffprobe", "-v", "quiet",
                            "-show_entries", "format=duration",
                            "-of", "csv=p=0", str(p)],
                           capture_output=True, text=True)
        return float(r.stdout.strip())

    seg_files = []
    for name, _ in SLIDES_FN:
        slide_png = SLIDES / f"{name}.png"
        narr_mp3 = NARR / f"{name}.mp3"
        seg = SEGS / f"{name}.mp4"
        d = dur(narr_mp3)
        seg_len = round(d + 1.0, 2)
        run([
            "ffmpeg", "-y",
            "-loop", "1", "-i", str(slide_png),
            "-i", str(narr_mp3),
            "-filter_complex",
            f"[0:v]scale=1920:1080:force_original_aspect_ratio=decrease,"
            f"pad=1920:1080:(ow-iw)/2:(oh-ih)/2,format=yuv420p,"
            f"fade=t=in:st=0:d=0.4[v]",
            "-map", "[v]", "-map", "1:a",
            "-t", str(seg_len),
            "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-r", "24",
            "-c:a", "aac", "-b:a", "160k", "-ar", "44100",
            "-shortest", str(seg),
        ])
        seg_files.append(seg)
        print(f"  segment {name}: narration {d:.1f}s → video {seg_len:.1f}s")

    print("\n=== concatenating ===")
    lst = SEGS / "list.txt"
    with open(lst, "w", encoding="utf-8") as f:
        for s in seg_files:
            f.write(f"file '{s.as_posix()}'\n")
    final = OUT / "t3n-sentinel-scf46-demo.mp4"
    run([
        "ffmpeg", "-y", "-f", "concat", "-safe", "0", "-i", str(lst),
        "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-r", "24",
        "-c:a", "aac", "-b:a", "160k", "-ar", "44100",
        str(final),
    ])
    sz_mb = final.stat().st_size / 1024 / 1024
    print(f"\nDONE  →  {final}  ({sz_mb:.1f} MB)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
