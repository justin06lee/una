# Korean and Korean–English code-switching

**Status: a plan, waiting for your approval. Nothing has been trained.** Written 2026-10-20.

You speak English and Korean, and you switch in the middle of a sentence: "오늘 PR 리뷰
끝나면 main에 merge 할게." una should write that down exactly as you said it, with Korean in
Hangul and English in Latin letters. It must never translate a span, transliterate English
into Hangul (merge → 머지), or romanize Korean (할게 → halge). English must not get worse.

This page covers what stops that today, the data and the training that would fix it, how
it's measured and gated, and what has to be decided before any GPU time is spent.

## What una does today

| Where | What happens | Effect on Korean |
|---|---|---|
| `asr.language = "en"` (config default, and every una.toml so far) | Whisper is told every dictation is English | Korean speech comes back as an English **translation**. On midna, "Okay, so I made a presentation." was stored for a Korean sentence (you excluded it) |
| Stock Whisper told `ko` | Decodes Korean well, but writes English words in Hangul | "this bug" → "이번 버그는 세션 매니지먼트 로직에", "cozy" → "코지" |
| Cleanup (`services/prompts.py`, llama3.2:3b) | English-only prompt and examples; Llama 3.2 doesn't officially support Korean | A full translation is caught by `max_divergence` and the raw text is pasted; translating one span isn't caught |
| `filters.py` | `normalize_text` keeps Hangul; edit distance is per character | Fixing a forced-English translation back to Korean scores ~1.0, so the pair is dropped as a "content rewrite". These are exactly the Korean pairs training would need |
| `train_lora.py` | One language prefix for every sample (`en`) | Korean targets would be trained under `<\|en\|>` |
| `evaluate.py`, Training page | WER only, one language | No CER for Korean, no mixed metric, no script check |
| Your history | 67 dictations since 2026-10-14 (midna's, now merged into tenet; tenet was reinstalled on 10-08, so older history is gone). None has Hangul in any text; language ID on the audio finds **1** Korean/mixed dictation | Almost no Korean of yours to train on |

## The approach

### One model, trained in two stages

**Stage A: a bilingual base, trained once.** A larger LoRA on public Korean, code-switched
and English speech plus your own recordings, merged into whisper-large-v3-turbo and kept as
a new base checkpoint (`una-koen-base`, HF format, about 1.6 GB). It is gated against stock
turbo on every eval set below. It runs again only when the data changes a lot.

**Stage B: your corrections, as today.** The existing correction LoRA keeps running on
your fixes. Only `base_hf_model` changes, to `una-koen-base`. A run is merged at conversion
as now, so serving still loads one CTranslate2 model. Its gate gains the Korean and mixed
sets, so a run trained mostly on English fixes can't wear the Korean away.

So: merged first, then stacked, but one model at serve time. Two models (one per language)
won't do, because a mixed sentence needs both at once and there's no VRAM for two plus the
cleanup model. Folding stage A into every correction run would turn a short run into hours.

### Which language token

Whisper picks one language token per 30-second window. The plan:

- **Training labels:** pure English goes under `<|en|>`. Anything with Korean speech in it
  goes under `<|ko|>`, with its English kept in Latin letters, whichever language carries
  the sentence. `<|ko|>` comes to mean "Korean, English words as English".
- **Serving: route each dictation.** Whisper's own language ID, narrowed to en and ko,
  picks `<|ko|>` when P(ko) / (P(ko) + P(en)) ≥ τ. τ is set low (around 0.15–0.3) so a
  sentence with any Korean goes down the Korean path, then tuned on dev data. English stays
  on `<|en|>`, which training leaves alone apart from replay.
- **Pilot option:** "Tell 엄마 I'll call her" is mostly English and may route to `<|en|>`.
  If the pilot shows that path romanizing the name, a small slice of English-led mixed
  sentences is trained under `<|en|>` as well.
- **Also measured:** prompting with both tokens (`<|ko|><|en|>`) lowered mixed error on
  Mandarin–English with no training ([Peng et al. 2023](https://arxiv.org/abs/2305.11095)).
  The baselines below test it zero-shot. It isn't the plan, because faster-whisper's
  long-audio path builds a one-token prompt.

## Data

### Public Korean speech

| Set | License | Size | Access | Use |
|---|---|---|---|---|
| [FLEURS](https://huggingface.co/datasets/google/fleurs) ko_kr | CC BY 4.0 | 7.9 h train (2,307 utts), 0.8 h dev, 1.3 h test (382) | Open, not gated | Train + dev; test is a KO gate set |
| [Zeroth-Korean](https://www.openslr.org/40/) (OpenSLR 40) | CC BY 4.0 | 51.6 h train (22,263 utts, 105 speakers, **only 3,000 sentences**); 1.2 h test (457) | Open, 10.3 GB | Train, capped at 2 recordings per sentence (~14 h) so it can't memorize 3,000 sentences; test is a KO gate set |
| [Emilia-YODAS](https://huggingface.co/datasets/amphion/Emilia-Dataset) ko | CC BY 4.0 (Emilia proper: CC BY-NC 4.0) | 7.3k h, machine-labelled | HF account + click-through | Optional: a ~30 h subset that the base model agrees with |
| Common Voice ko | CC0 | 2.5 h validated | Mozilla Data Collective account | Low value; skip unless you have an account |
| [YODAS](https://huggingface.co/datasets/espnet/yodas) ko | CC BY 3.0 | 2.8k h "manual" YouTube captions | Open, but its loader script no longer runs | Skip: caption timing and quality too uneven |
| KsponSpeech, AI Hub 자유대화/회의/명령어 | AI Hub terms | 1k–4k h each | **Korean nationals only**, approval, and taking data out of Korea needs a separate agreement with NIA | Probably unusable from the US. Never use the HF re-uploads of AI Hub data (`*ksponspeech*`, `aihub-*`): they break AI Hub's no-redistribution rule |
| Pansori-TEDxKR, KSS, ClovaCall | CC BY-NC-ND, CC BY-NC-SA, research-only form | 3–125 h | Varied | Skip: licenses or domain |

### Public code-switched (Korean–English)

| Set | License | Size | Use |
|---|---|---|---|
| [HiKE](https://huggingface.co/datasets/thetaone-ai/HiKE) | Apache-2.0, not gated | **Test only**: 1,121 utts, 2.2 h, 13 speakers, labelled word/phrase/sentence switches and loanwords | Mixed gate set: split 30% dev (tune τ, pick pilot variants), 70% test (gate) |
| [CS-FLEURS](https://huggingface.co/datasets/byan/cs-fleurs) ko-en | CC BY-NC 4.0 | 2,211 synthetic (XTTS) train clips, 466 human-read test | Optional, non-commercial: extra mixed eval, a little synthetic training |
| AI Hub 한영 혼합 인식 (71260) | AI Hub terms | 14,013 h natural mixed speech | The one large natural set, but nationals-only plus the export rule. Only if you're eligible |

No openly licensed natural Korean–English **training** set exists. HiKE's authors measured
why that matters: fine-tuning Whisper on natural code-switched speech took mixed error
from 31.3 to 9.0, while monolingual clips spliced together only reached 22.1
([HiKE](https://arxiv.org/abs/2509.24613)). Natural mixed speech in your voice is the most
valuable data this plan can get, which is why the recording request below has an optional
training part.

### Your own dictations

- All of it: 67 dictations since 2026-10-14, recorded on midna and merged into tenet's
  database. tenet was reinstalled on 10-08, so nothing older survives, including the
  September style model.
- None has Hangul in any text. Language ID on the audio found 1 that is Korean or mixed
  (01M56K831…, 11 s, which you excluded), plus one hallucination on near-silence.
- Going forward: once routing ships, your Korean dictations get Korean transcripts and
  your corrections become training pairs. `filters.py` needs a "script fix" rule for this
  (below).

### Synthetic code-switched audio

1. **TTS in a consistent voice.** Pilot two Apache-licensed models that take mixed
   Korean/English text: MOSS-TTS ([OpenMOSS-Team](https://huggingface.co/OpenMOSS-Team);
   Korean listed, code-switching claimed, so far only on third-party pages) and
   Qwen3-TTS 1.7B, which has `language="Auto"`. You listen to about 20
   samples from each before anything is generated at scale; English inside Korean
   sentences must come out as English, not Konglish. Voices: a clone from your recordings
   (your decision) plus a handful of stock voices.
2. **Text.** An LLM writes mixed sentences from templates in your domains (dev work,
   agents, daily life), seeded with your dictionary entries. They're deduplicated against
   every eval sentence (HiKE and your 50) by 4-gram overlap.
3. **Same-speaker splicing (optional).** If you record Korean training sentences, English
   phrases from your own dictations can be cut in at word boundaries: same mic, same room,
   loudness-matched, 10 ms crossfades. Don't splice across different speakers: the model
   would learn "voice change = language change".

**Keeping artifacts out:**
- TTS is capped at 20% of training samples overall and never appears in an eval set.
- Every TTS clip is round-tripped through stock turbo; clips whose Korean spans come back
  with CER > 15% or whose English spans have WER > 30% are dropped. That catches garbled,
  clipped or Konglish audio.
- Training audio is augmented: room responses ([RIRS](https://www.openslr.org/28/),
  Apache 2.0), noise ([MUSAN](https://www.openslr.org/17/), CC BY 4.0) at 10–30 dB SNR,
  ±10% speed, and codec compression, so a clean TTS spectrum isn't a cue.
- The mixed gate is real speech only. A model that improved only on TTS would show no
  gain.

### Labels in Whisper's own style

Whisper writes cased, punctuated text with digits; una's correction targets do too. Not
one of Zeroth's 22,263 training transcripts has a digit, a punctuation mark or a Latin
letter: numbers are spelled out in Hangul. LibriSpeech is uppercase without punctuation.
Training on them raw would teach the model to drop punctuation and spell out numbers. So
each reference keeps its own words but takes Whisper's punctuation and casing, copied over
by aligning it with stock turbo's output for the same clip. That costs one GPU pass, about
30–45 min for ~22 h of audio.
FLEURS (raw transcription) and HiKE are already punctuated. Numbers can't be transferred
that way, so a Zeroth clip is dropped when stock turbo's output for it has a digit. That
loses perhaps 10–20% of Zeroth.

### The mix (stage A, by sample)

| Share | What | Token |
|---|---|---|
| 20% | English replay: FLEURS en train, optionally a punctuated LibriTTS-R subset. Not your own English: that is held out as EN-YOU (below) | `<\|en\|>` |
| 30% | Korean: Zeroth (capped), FLEURS ko train, optional Emilia-YODAS subset | `<\|ko\|>` |
| 40% | Mixed: your training recordings (oversampled up to 5×), TTS (≤ half of this share), CS-FLEURS if allowed | `<\|ko\|>` |
| 10% | Your Korean dictations, once routing produces them | `<\|ko\|>` |

The 20% English share follows Whisper replay results: about 10% replay already protected
general WER, and at 20% the new domain started to suffer
([Replay to Remember](https://arxiv.org/abs/2307.07280)). Here English must not regress at
all, so the pilot starts at 20% and moves to 30% if the English gate slips. Short clips
can be packed into ~28 s windows for speed (see Compute). Your own clips are never packed.

## Evaluation sets

| Set | What | Gate metric |
|---|---|---|
| **MIXED-YOU** | Your ~50 recorded mixed sentences (request below). Never trained on | MER + script check |
| MIXED-HiKE | HiKE test split (~780 utts), the 70% not used for tuning | MER + script check |
| KO-FLEURS, KO-Zeroth | FLEURS ko test (382), Zeroth test (457) | CER + script check |
| **EN-YOU** | For stage A, all 42 of your eligible English pairs (1.4 + 10 min of audio). una's frozen crc32 holdout has only 4 of them, below the gate's 40, so stage A trains on none of yours. Stage B goes back to the crc32 split once it has 40 | WER, as the gate today |
| EN-FLEURS | FLEURS en_us test (647) | WER |

**Metrics** (`server/scripts/koen/koen_metrics.py`):
- **WER** uses una's normalization, the same as today's gate.
- **CER** counts Hangul syllables and Latin letters with spaces removed; Korean spacing is
  loose enough that word scores mostly measure the spaces.
- **MER** treats one Hangul syllable or one Latin word as one token, the usual unit for
  code-switched ASR.
- **Script violations**: a reference token that comes back only in the other script (회의
  → meeting, merge → 머지, 안녕 → annyeong). Ordinary misrecognitions within a script don't
  count. Reported as utterances with any violation.

## Gates

Every number is measured on tenet's GPU with the serving `compute_type`, greedy decoding,
the same harness for candidate and baseline, and routing as it would serve. The **baseline
is stock turbo with its best language setting for each set**: forced `en` for EN, forced
`ko` for KO, and Whisper's own detection for mixed (measured below). Comparing against
today's forced-English behaviour would make any model look good; the gate should measure
what the training did. Stage A promotes only
if **all** of these hold:

| Set | Must reach | Why |
|---|---|---|
| EN-YOU | WER ≤ baseline (Δ ≤ 0), n ≥ 40 | English never regresses on your own speech |
| EN-FLEURS | WER Δ ≤ +0.3 points | Catches general English damage; the 0.3 allows about one word in 300 of decode noise (your call: could be 0) |
| KO-FLEURS, KO-Zeroth | CER Δ ≤ 0 on each; script violations no more than baseline (stock: 2 of 40 on FLEURS, NextGen → 넥스트젠 and km → 키로미터) | Korean can't get worse while mixed gets better |
| MIXED-HiKE | MER ≤ 0.75 × baseline (≥ 25% relative gain); violating utterances ≤ 5% and below baseline | The goal, on a public set |
| MIXED-YOU | MER ≤ 0.75 × baseline; violating utterances ≤ 3 of 50 and below baseline | The goal, in your voice |
| Routing | ≥ 99% of EN-YOU routed to `<\|en\|>`; ≥ 97% of MIXED routed to `<\|ko\|>` | Only applies if the two paths differ on English |

Stage B (correction runs on top of `una-koen-base`) keeps today's rule: EN-YOU WER must
improve by `promotion_margin_wer` on ≥ `min_eval_samples`. It adds non-regression on KO CER
(Δ ≤ +0.3), on MIXED-HiKE and MIXED-YOU MER (Δ ≤ +0.5), and on script violations
(no more than baseline).

### Baselines (stock large-v3-turbo, today)

Measured on the Mac's CPU (int8), on random subsets (seed 0), because tenet's GPU is busy
with another job. CPU int8 differs slightly from GPU float16; the gate
measures again on tenet.

Lower is better. "Script" is the share of utterances with at least one span in the wrong
script.

| Set (n) | `en` forced (una today) | `ko` forced | auto (Whisper's ID) | `<\|ko\|><\|en\|>` |
|---|---|---|---|---|
| HiKE mixed (60) | MER 53.8, script 43% | MER 36.1, script 57% | **MER 26.7**, script 55% (18 → en, 42 → ko) | MER 35.5, **script 40%** |
| FLEURS ko (40) | CER 307 (translated), script 98% | **CER 2.7**, script 5% | not run | not run |
| FLEURS en (40) | **WER 6.3**, script 0% | WER 95 (hallucination loops) | not run | not run |
| EN-YOU (42) | **WER 5.0**, script 0% | not run | WER 5.0, all 42 → en | not run |
| MIXED-YOU, KO-Zeroth | pending: your recordings; a GPU run | | | |

What this shows:
- Today, any Korean you say comes out as English.
- Stock Whisper already hears Korean well (CER 2.7 when told `ko`). The problem is
  English inside Korean: told `ko`, it transliterates (patient → 페이션트, scanning
  electron microscopy → 스캐닝 일렉트론 마이크로스커피). Told `en`, it translates the Korean.
- Forcing `ko` on English speech is unusable (WER 95), so routing is required, and the
  `<|en|>` path has to stay as it is. Whisper's own detection sent all 42 of your English
  dictations to `en`, so routing doesn't touch your English.
- 31 of EN-YOU's 42 references are transcripts you accepted unchanged, so any change from
  stock output counts as an error there. That makes "Δ ≤ 0" strict, as today's gate is.
- Even the best zero-shot setting leaves at least one span in the wrong script in 40–55%
  of mixed sentences. That is the gap training has to close.

So the mixed gate's baseline is auto (MER 26.7 on this subset). Promotion needs MER ≤ 20 on
the full HiKE test split, and script violations under 5% of utterances, against 40–55%
now. Each set is re-measured on tenet's GPU before the gate.

## What else in una has to change

| Layer (owner) | Change |
|---|---|
| Server services: transcriber, config | `asr.language = "route"` with `asr.route_languages = ["en", "ko"]` and `asr.ko_threshold`. Detection should reuse the encoder output for the decode. faster-whisper 1.2.1's own auto-detect doesn't: it encodes the first window twice (its `generate_segments` takes an `encoder_output`, which allows the reuse). Measure the cost on tenet either way. Store the detected language and P(ko) in `dictations.language` |
| Server services: cleanup | Never change script. If the raw text has Hangul, the cleaned text must have zero script violations against it (reuse `koen_metrics.script_violations`), otherwise paste raw. Add Korean fillers (음, 어, 그, 막, 약간) and a "keep each word in the language and script it was spoken in" rule with one mixed example. Llama 3.2 3B doesn't support Korean, so the first version should **skip cleanup for Hangul transcripts** until a cleanup model that handles Korean (e.g. qwen3:4b) is measured |
| Training loop (me) | `Sample.language` and a per-sample prefix in `train_lora.py`. A stage-A run kind with its own LoRA targets (decoder q/k/v/o/fc1/fc2 + encoder q/v). Per-set metrics and the gates above in `evaluate.py`/`promote.py`, moving `koen_metrics.py` into `training/` with tests. Migration 009: `training_runs.metrics_json`. A "script fix" eligibility rule in `filters.py`: raw is Latin-only, the correction is mostly Hangul and the audio's P(ko) ≥ τ → eligible, instead of "content rewrite". `norm_edit_distance` already works on Hangul |
| Server API | `language` in dictation responses becomes meaningful; no other change |
| Dashboard: training, home | Per-set metrics on the Training page (EN WER, KO CER, mixed MER, script violations) instead of one WER line; each dictation's language in history |
| Desktop shell, platform inject | Pasting Hangul through the pasteboard should just work. **Needs checking:** the post-paste edit watcher with the Korean IME (composition and marked text, keyDown counting) and character offsets (`correction.rs` measures byte lengths; Hangul is 3 bytes in UTF-8 but 1 unit in the UTF-16 ranges AX reports) |
| Teacher labels, writing import | English prompts; skip both for Hangul dictations for now |

## Compute

- **GPU, confirmed 2026-10-20:** NVIDIA GeForce GTX 1660 Ti, 6144 MiB. That's Turing:
  fp16 without tensor cores (about 11 TFLOPS peak), no bf16. When checked, another job
  held 4057 MiB at 100% utilization, so nothing for this plan runs until it ends. The host
  has 16 cores and 15 GB of RAM.
- **Does it fit:** yes, with serving paused (`training.pause_serving`). fp16 weights
  (1.6 GB), LoRA and Adam state (~0.2 GB), and activations with gradient checkpointing at
  batch 2 (1.5–2.5 GB) come to about 4–4.5 GB of 5.6.
- **Time:** Whisper encodes a full 30 s window however short the clip. Back of the envelope
  for full LoRA (encoder included): ~1.5–2.5 s per window. With ~30 h of training audio
  in ~6 s clips (18k clips), one epoch unpacked would take 8–12 h, which is too long. Two
  levers, both tried in a 20-minute pilot on 1k windows before committing:
  - **Packing** clips into ~28 s windows (~4×): 2–3 h per epoch.
  - **Decoder-only LoRA with cached encoder outputs:** one encoder pass per clip, then
    epochs cost minutes. About 4 MB per window on disk (~75 GB for 20k windows).
- **Budget:** one overnight GPU slot for stage A (pilot 0.5 h, training ≤ 6 h, conversion
  and every gate 0.5 h). Before that, ~1 h of GPU prep: punctuation transfer, the baseline
  on GPU, and the TTS pilot. A full TTS run of 5–10 h of audio is another 2–5 h.
- **Disk:** ~16 GB downloads (FLEURS ko/en 3.6 GB, Zeroth 10.3 GB, HiKE 0.24 GB), about
  the same unpacked; optional MUSAN 11 GB, RIRS 1.3 GB, LibriSpeech-100 6.3 GB; up to 75 GB
  of encoder cache. tenet has 812 GB free.

## Prep that needs no GPU

Done, on the Mac (CPU and disk only; scratch at `/Volumes/T7/Stockpile/scratch/una-koen/`):
- The eval harness and metrics, `server/scripts/koen/`:
  - `koen_metrics.py`: WER, CER, MER, script violations.
  - `koen_eval.py`: scores a CT2 model on manifests under en/ko/auto/route/ko+en.
  - `koen_data.py`: manifests for FLEURS, Zeroth and HiKE, and a read-only catalogue of
    una's DB with language ID on the audio.
  - `fetch_public.sh`: idle-priority, resumable download of the open sets, recording
    licenses and checksums.
- `fetch_public.sh` run end to end on the Mac:
  - 15 GB under `public/`: FLEURS ko/en (all splits), HiKE, Zeroth.
  - Manifests match the published counts.
  - `SOURCES.tsv` records each set's URL, license, sha256 and fetch time.
  - It went to tenet over the LAN instead of being downloaded again, as
    `COPYFILE_DISABLE=1 tar -cf - . | ssh … 'ionice -c 3 nice -n 19 tar -xf -'`. Without
    `COPYFILE_DISABLE`, macOS tar adds `._*` files, which broke the Zeroth reader. Running
    `fetch_public.sh` there afterwards skips every finished download and rewrites the
    manifests for tenet's paths.
- The baselines above.
- midna's dictations catalogued from a read-only copy.

On tenet, CPU and disk only, at idle priority:
- The same 15 GB is at `~/una-koen/`, copied from the Mac over the LAN.
- The scripts are at `~/una-koen/scripts/`, outside the checkout, until this commit is
  pulled there.
- Manifests were rebuilt for tenet's paths in `~/una-koen/manifests/`, with the same
  counts as the Mac's. The GPU job held 4057 MiB at 100% throughout.
- Your history wasn't catalogued again: tenet's database holds the same 67 dictations as
  midna's.

Next, once approved, still without a GPU:
- The code changes in the training loop and their tests.
- The text for the TTS pilot.
- Splitting HiKE into dev and test.
- Packing and caching tooling.

## Decisions for you

1. **Approve the plan:** stage A on top of turbo, routing, the mix, the gates.
2. **Loanwords.** Proposal: a word you say the English way is written in Latin (meeting,
   deploy); a word you say as a Korean word is written in Hangul (미팅 if you say 미팅).
   In short, write it the way you'd type it. This decides the labels for your recordings
   and the synthetic text.
3. **Record** the 50 eval sentences. Optionally also 200–300 training sentences (~25 min),
   the most valuable data in this plan, and 20 pure Korean sentences.
4. **Non-commercial data** for a personal model: CS-FLEURS (CC BY-NC) and Emilia proper
   (CC BY-NC). The trained weights would stay private.
5. **Accounts** (each optional): a Hugging Face login for Emilia-YODAS' click-through, and
   a Mozilla Data Collective account for Common Voice. AI Hub only if you're a Korean
   national and the export rule is worked out.
6. **TTS:** whether to clone your voice for synthetic data, and which model, after
   listening to samples.
7. **Cleanup for Korean:** skip cleanup for Hangul transcripts at first (recommended), or
   switch the cleanup model.
8. **EN-FLEURS tolerance:** +0.3 points, or a strict 0.
9. **GPU slots on tenet:** a ~1 h prep slot and one overnight training slot, with
   dictation paused during them.

## The recording request

This is the text to send you. It's also the brief for anyone helping you record.

> **Recording 50 mixed sentences for una's Korean test set**
>
> These become the test that decides whether a Korean-English model is good enough to
> replace the one you use now. They're never trained on, so they need to sound like you,
> not like a script.
>
> **Writing them.** Use the 50 below as they are, or rewrite any of them in your own words
> (better). Aim for things you'd actually dictate: messages, notes, prompts to coding
> agents. Keep roughly this mix:
> - ~20 Korean sentences with English words in them, including English words with Korean
>   particles attached (server에서, PR을).
> - ~10 with English phrases (make sense, by default, good enough).
> - ~8 that switch Korean → English → Korean across clauses.
> - ~7 English sentences with Korean words (names, food, places).
> - ~5 with numbers, times, versions or money.
>
> Make about a third short (under 4 s), a third medium, and a third long (10–20 s); none
> over 30 s. Write each exactly as you'd want it typed: Korean in Hangul, English in Latin
> letters, your own spelling, spacing and punctuation. A loanword is written the way you'd
> type it: if you say 미팅, write 미팅; if you say "meeting", write meeting.
>
> **Recording.** Record through una itself, so it's your usual mic and the same path as
> real dictation:
> 1. Open an empty note to paste into. una still assumes English, so what it pastes will
>    be wrong. That's expected; don't edit it, and dismiss any correction window.
> 2. Hold the hotkey, say sentence 1 the way you'd naturally say it, at your normal speed,
>    release. One take each. If you said it differently from the text, change the text to
>    match what you said rather than recording again.
> 3. Send back the final list (numbered, as text) and roughly when you recorded it. We'll
>    move those dictations out of una's history into the test set and exclude them from
>    training.
>
> **Optional, and the most useful thing you could add:** 200–300 more sentences of the same
> kind (~25 min), recorded the same way, used for **training**. Natural mixed speech in your
> own voice helps far more than anything synthetic. We can draft them for you to edit.
> Also optional: 20 pure Korean sentences.
>
> **The 50 (edit freely):**
>
> *Korean with English words*
> 1. 오늘 PR 리뷰 끝나면 main에 merge 할게.
> 2. 이 function이 왜 undefined를 return 하는지 좀 찾아 줘.
> 3. 서버 log 보니까 timeout이 계속 나더라.
> 4. Slack에 링크 올려놨으니까 확인해 봐.
> 5. 이번 sprint에는 dashboard 쪽 bug부터 고치자.
> 6. 그 API key는 env 파일에 넣어 두면 돼.
> 7. 내일 standup 몇 시였지?
> 8. Claude한테 test 다 돌려 보라고 했어.
> 9. commit 하기 전에 lint 한 번만 돌려 줘.
> 10. GPU memory가 부족해서 batch size를 반으로 줄였어.
> 11. 지금 Docker image 다시 build 하는 중이야.
> 12. 회의 끝나고 Figma 파일 공유해 줄 수 있어?
> 13. 이 부분은 그냥 hardcode 해도 될 것 같아.
> 14. README 다시 읽어 봤는데 install 부분이 좀 헷갈려서, 처음 쓰는 사람 입장에서 step by step으로 다시 써 보는 게 좋을 것 같아.
> 15. 이번 주말에 Costco 가서 장 보자.
> 16. 그 영상 YouTube에 올렸어?
> 17. Jetson에 SSH로 들어가서 service 다시 시작해 줘.
> 18. 이 branch는 conflict가 너무 많아서 rebase 하기 싫다.
> 19. 새 laptop 오면 setup 하는 데 하루는 걸릴 것 같은데, 그 전에 backup 먼저 해 두고 dotfiles도 GitHub에 올려 놔야겠다.
> 20. 엄마가 iPhone 업데이트하고 나서 사진이 안 보인대.
>
> *English phrases inside Korean*
> 21. 그거 그냥 skip 하고 다음 거 먼저 하자.
> 22. 이건 완전 edge case인데 일단 handle은 해 놓자.
> 23. 나 지금 좀 busy 하니까 이따가 call 할게.
> 24. 그 옵션은 by default로 꺼져 있어.
> 25. 일단 make sense 하는지 한번 봐 줘.
> 26. 결과가 너무 random 하게 나와서 seed를 고정했어.
> 27. 이 정도면 good enough 아니야?
> 28. 솔직히 그 사람 말이 basically 다 맞아.
> 29. 오늘 하루 종일 back to back으로 meeting 있어서 점심도 제대로 못 먹고 아무것도 못 했어.
> 30. 그건 나중에 follow up 하자, 지금은 시간이 없어.
>
> *Korean → English → Korean*
> 31. 내일 오전에 standup 있는데, I'll probably be a bit late, 그러니까 먼저 시작해.
> 32. 이거 다 끝났어? Because I need to push it tonight. 아니면 내일 해도 되고.
> 33. 아까 말한 거, the one about the memory leak, 그거 해결됐어.
> 34. 나 지금 나가는데, can you lock the door? 열쇠는 책상 위에 있어.
> 35. 숫자가 좀 이상한데, maybe it's a caching issue, 한번 확인해 볼게.
> 36. 점심 뭐 먹을래? I'm thinking ramen, 아니면 김밥도 괜찮고.
> 37. 이거 리뷰해 줘. No rush though, 다음 주까지만 해 주면 돼.
> 38. 오늘 진짜 피곤하다. I just want to sleep, 근데 할 일이 너무 많아서 오늘 밤에 좀 더 해야 될 것 같아.
>
> *English with Korean words*
> 39. Can you send me the 회의록 from yesterday's meeting?
> 40. I'm having 삼겹살 with 민수 tonight, so I'll be offline.
> 41. Tell 엄마 I'll call her after work.
> 42. The 택배 came today, it's on the porch.
> 43. Let's get 떡볶이 after the gym.
> 44. I told 지훈 the deadline is Friday, 근데 아직 답이 없어.
> 45. Remind me to pay the 관리비 before the end of the month.
>
> *Numbers, times, versions, money*
> 46. 3시 반에 meeting 있으니까 2시 50분까지 와.
> 47. 이번 달 AWS bill이 230달러나 나왔어.
> 48. version 2.4.1로 올리고 나서부터 계속 crash가 나.
> 49. The flight is at 7:45, 그러니까 5시에는 출발해야 돼.
> 50. 하루에 dictation을 한 50개 정도 하는 것 같아.
