# 0005 — Local SLM Model Selection, Licensing, Quantization, and Determinism Reference

> **Status**: Proposed — not yet implemented. Reference material for Phase 4 of
> `vyapaar-agent/docs/plans/0004-free-tier-llm-backend-and-deferred-local-slm.md`.
> **Target Crates**: `vyapaar-agent-llm` (Phase 4 — new `LlamaCppProvider`/`VllmProvider` impls).
> **Depends on reading**:
> - `vyapaar-agent/docs/plans/0004-free-tier-llm-backend-and-deferred-local-slm.md`

---

## 0. Executive Summary & Objective

`0004`'s Phase 4 names the local-SLM tiers to adopt (Qwen3-4B/SmolLM3-3B, Qwen3-8B/14B) at a
planning level but doesn't carry the supporting detail behind those choices. That detail — model
licensing against this workspace's AGPL-3.0 exposure, why constrained decoding alone isn't enough,
how much fine-tuning data each workload actually needs, which quantization level is safe, and why
none of the three local-serving engines (llama.cpp/Ollama/vLLM) actually guarantee the determinism
`vyapaar-agent-replay`'s SHA-256 hashing assumes — originally lived in a deep-research report and
four research-note files that are being removed from the repository as scratch material now that
their substance is captured here. This doc is that capture, scoped specifically to what `0004`'s
Phase 4 needs and doesn't already say.

---

## 1. Model licensing against this workspace's AGPL-3.0 exposure

A separate open-source product this workspace was compared against (`FinceptTerminal`) is
AGPL-3.0-licensed; nothing in `vyapaar-labs` itself currently carries that license, but the
comparison raised the question of which base-model licenses are safe to combine with AGPL-licensed
code elsewhere in a stack, in case this workspace or a derivative of it ever does.

| Model family | License | AGPL-3.0 interaction |
|---|---|---|
| Qwen3 (0.6B–14B dense + MoE) | Apache 2.0 | Clear — permissive, no copyleft/network-use clause, legally separate artifact from any AGPL component |
| SmolLM3-3B | Apache 2.0 | Clear |
| Mistral Nemo 12B / Mistral Small 3 24B | Apache 2.0 | Clear |
| IBM Granite 4.0 Nano | Apache 2.0 | Clear |
| Phi-4-mini | MIT | Clear |
| Llama 3.2 (1B/3B) | Llama Community License | Not copyleft, but carries real contractual overhead: mandatory "Built with Llama" notice, retained attribution file, "Llama" required in any distributed fine-tune's name, an incorporated Acceptable Use Policy, and a monthly-active-user threshold above which Meta's separate commercial license applies at its sole discretion |
| Gemma 3 (1B/4B/9B/12B) | Gemma Terms of Use | Not copyleft, but requires redistributing the use-restriction terms, marking modified files, shipping Google's attribution notice; no trademark grant; termination clause requires deleting all copies |

Practical takeaway: prefer Qwen3/SmolLM3/Mistral/Granite (Apache 2.0) or Phi-4-mini (MIT) over
Llama or Gemma when a comparable model exists in both camps — not because of a copyleft conflict
(there isn't one either way), but because the Apache/MIT options carry materially less compliance
overhead. One genuinely unsettled question, independent of any specific model here: whether AGPL's
network-use clause could ever be read to extend to model weights trained or served with
AGPL-licensed tooling (a position Ultralytics has publicly taken for YOLO weights, disputed by other
community discussion). This depends on what this workspace's own code licenses as AGPL, not on any
base model's license — a question for counsel if it ever becomes live, not resolved here.

Tool-calling note: Gemma's own documentation states it emits no tool-specific tokens and enforces
no grammar on its own — a serving framework must detect compliance by pattern-matching the prompted
format. This disqualifies Gemma as a *sole* solution for `0004`'s closed-grammar decision parser
regardless of license. Qwen3's "native" tool-call parser mode has at least one reported failure
under vLLM (empty arguments); the practical convention is to standardize on a Hermes-style tool-call
parser (`--tool-call-parser hermes` in vLLM) across models rather than trusting each model's native
format.

---

## 2. Constrained decoding and fine-tuning are complements, not substitutes

Grammar-constrained decoding (llama.cpp GBNF, Outlines, vLLM's XGrammar-backed structured output)
masks invalid tokens before sampling, which makes it structurally impossible for the model to emit
a token that breaks the schema. This is mature enough to effectively guarantee `0004`'s Decision
enum *parses*. It does not guarantee the *value* chosen is correct: one practitioner benchmark found
constrained decoding scored **lower** accuracy than free-form generation (91.37% vs 93.63%) on the
same test set, including a case where the constrained output confidently returned the wrong value
rather than the correct one. A separate study found instruction-tuned models can show measurable
*performance degradation* under grammar constraints, because the constraint forces the model away
from its natural, highest-confidence output pattern into a lower-confidence structured alternative.

The strongest evidence that fine-tuning and constrained decoding are complements: a fine-tuned
Mistral-7B combined with constrained decoding reached 99.5% schema accuracy and 94.0% content
similarity in one study (the "SLOT" approach), describing the pairing as a "powerful synergy."
**Implication for `0004`'s Phase 4 Tier 1**: fine-tune the chosen small model on the exact closed
schema specifically, so its natural output distribution already aligns with the grammar, rather
than relying on a generic instruction-tuned base plus grammar alone.

Data-size guidance (no source gives a number for closed-enum classification specifically — this is
an inference by analogy, not a cited figure): LIMA's result (1,000 curated examples producing
outputs preferred over GPT-4 in 43% of comparisons) supports a "few hundred to ~1,000 well-curated,
class-balanced examples" target for Tier 1's narrow schema. Tool-calling fine-tunes for Tier 2
operate at a different scale — published datasets run from 10K (a practical subset shown sufficient
in one tutorial) up to 60K–80K for broad/foundational tool-calling capability — so Tier 2 should
target low-to-mid thousands of examples against `vyapaar-agent-chat`'s actual tool registry, not
LIMA-scale hundreds.

Synthetic data generation: this project's own deterministic risk-clamping engine
(`vyapaar_risk::RiskEngine::check_decision`) can serve as the verification oracle for Tier 1's
synthetic training data — generate (market/portfolio context → Decision) pairs from a frontier
teacher model, then accept into the training set only examples the oracle would also validate as
correctly formed. This closes the literature's main warning about distillation (a student model
inherits a teacher's errors, since there's usually no ground-truth check available) using an oracle
this project already has, rather than trusting the teacher's self-consistency alone. The equivalent
check for Tier 2's tool-calling data is executing each generated call against a sandboxed version
of the real read-only tool and verifying the result, not just checking the teacher's JSON is
well-formed.

---

## 3. Quantization: Q4 is a floor to validate, not a default to trust

Memory footprint at Q4_K_M is comfortable at every consumer hardware tier named in `0004`: roughly
1.8–2.5GB for a 3B model, 4–5GB for 7B, 8.5–10GB for 14B — well inside an 8GB GPU and far inside
16–32GB of system RAM. Memory and raw throughput are not the binding constraint.

The binding constraint is that quantization degrades structured-output and tool-calling
reliability in ways standard benchmarks hide. A NeurIPS 2026 workshop poster found a 1.7B model's
missed-required-tool-call rate nearly **tripled** under 4-bit quantization (6.0% → 17.3%), while a
4B model from the same family showed **no measurable effect** at the same bit-width — a
statistically significant size-dependent interaction, not a fixed threshold. The authors' own
conclusion: *"4-bit safety should be validated per checkpoint instead of assumed from a size
threshold."* A separate multi-turn agent study found 4-bit quantization's effect on the *headline*
task-score metric stayed within ±7.5 points (looking essentially free), while per-call failure
analysis showed tool-name hallucinations increasing up to 2.5x in volume — the aggregate metric was
quietly absorbing the damage. One vendor benchmark recommends avoiding anything below Q3 for
structured output specifically, since 1-bit tool-call validity fell to around 36%.

**Practical rule for `0004`'s Phase 4**: treat Q4_K_M as the usable floor, not a safe default.
Before any quantized local model replaces the free-tier router for a given workload, run that exact
checkpoint's quantized build against the Phase 3 golden fixtures and confirm structured-output/
tool-calling reliability directly — do not infer it from the model's unquantized benchmark scores
or from another model's quantization behavior at a similar size. Where available, an official
quantization-aware-training (QAT) checkpoint (Google shipped one for Gemma 4 in mid-2026) or a
reputable community "dynamic" quantization (Unsloth's Dynamic 2.0/3.0 lineage) is plausibly safer
than a naive uniform GGUF quant at the same bit-width — though no source found during this research
directly measured that improvement on tool-calling reliability specifically, only on general
perplexity, so this is a candidate to test, not an assumed win.

---

## 4. Determinism: no engine guarantees it from seed + temperature=0 alone

This is the most consequential correction to `0004`'s design for anyone touching
`vyapaar-agent-replay`'s SHA-256 hashing once a local engine is involved. **"Same prompt + fixed
seed + temperature=0 ⇒ byte-identical output" is not a safe assumption for llama.cpp, Ollama, or
vLLM.** vLLM's own documentation states plainly that it does not guarantee reproducible results by
default, and that even its recommended settings hold only on the same hardware and the same vLLM
version. A primary research write-up (Thinking Machines Lab) ran one prompt 1,000 times at
temperature=0 against vLLM and got 80 unique completions — not from random GPU math, but because
the batch of concurrent requests a given call lands in varies with server load, and reduction-heavy
kernels (RMSNorm, matmul, attention) choose different numerically-equivalent-but-differently-rounded
reduction strategies depending on batch composition. Applying purpose-built batch-invariant kernels
collapsed all 1,000 completions to identical output — the mechanism is fixable, but only with
engine-specific work (vLLM ships `VLLM_BATCH_INVARIANT=1`, beta, at roughly 1.6–2.1x the throughput
cost), not by seed-pinning alone.

llama.cpp has an analogous problem in CPU-only serving: maintainer-acknowledged GitHub issues show
the same seed on the same machine produces different output purely as a function of thread count
(`-t 14` vs `-t 15`), because `ggml`'s tensor-op work-splitting changes with thread count and
floating-point accumulation then rounds differently. Full determinism on CPU needs both llama.cpp's
own thread flag **and** the underlying BLAS library's threading pinned (e.g. `OMP_NUM_THREADS=1`) —
one is not sufficient alone. Ollama is built on llama.cpp's engine and documents no stronger
guarantee than "set seed for reproducible outputs," with no hardware/version caveat — the same
thread-count risk is a plausible latent issue there too, though no Ollama-specific reproduction was
found.

**Practical implication, directly actionable for `0004`'s Phase 1 (`LlmAuditRecord`) and Phase 4**:
the audit/replay record must capture the full serving-stack fingerprint, not just prompt and
completion — engine binary/version, the exact model file hash (not a model-name tag), the
quantization build, hardware identity, and (for CPU paths) thread count plus every BLAS/OMP
threading environment variable; for vLLM specifically, whether `VLLM_BATCH_INVARIANT=1` was set and
whether async scheduling was disabled. For a single-user, single-request-at-a-time deployment —
which matches this project's likely usage pattern — llama.cpp's simpler non-continuous-batching
execution model sidesteps vLLM's dominant non-determinism source entirely, at the cost of needing
careful thread/BLAS pinning instead of a batching fix. This makes llama.cpp the structurally safer
default for the replay-determinism requirement specifically, with vLLM's batch-invariance mode as
the fallback only where vLLM's throughput/batching advantages are needed (Tier 2/3's chat/research
workloads, per `0004`).

Two things remain genuinely unverified rather than just unresolved: whether quantization level
itself changes determinism risk (no source tested this directly — only BF16-vs-FP32 precision was
tested, which suggests but doesn't prove lower-bit quantization carries equal-or-greater
reduction-order sensitivity), and whether model size changes determinism risk (the one spot-check
available, 32B vs 7–8B, found the larger model has the *same* reproducibility challenges, not
better or worse ones).
