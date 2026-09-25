# Your personal model

una ends up with two models that are yours: Whisper tuned to how you sound, and a cleanup
LLM tuned to how you write. Both are trained on pairs of *what came in* and *what it should
have been*. This page covers where those pairs come from and how the cleanup model is
trained on them. Whisper's side is in the main README.

## Two targets per dictation

Every dictation has two right answers, and they are kept apart on purpose:

| | What it is | Trains |
|---|---|---|
| **Literal** | Exactly what you said, fillers and restarts included | Whisper (audio → literal) |
| **Polished** | What you would have typed had you written it | The cleanup LLM (transcript → polished) |

Mixing them up damages both. A polished text used as a Whisper target teaches it to stop
hearing your "um"s and self-corrections, and a literal one used as a style target teaches
the cleanup model to leave them in. So an edit you make to pasted text (which was the
*polished* version) only ever becomes a polished target, and the literal is confirmed in
review.

## The teacher

Confirming two texts per dictation by typing them out would never happen. The teacher
drafts both, so review is mostly pressing ↵.

It runs in the background after every dictation, never on the dictation path:

1. **A second listen.** A slower, more accurate Whisper (`large-v3` with beam search by
   default, on the CPU so it never competes with serving for VRAM) transcribes the stored
   audio again, with word timings. Wherever it heard different words from the transcript
   una served, that span is recorded along with where in the audio it happened, so review
   can mark it and play just that moment.
2. **A reconciliation.** If an LLM endpoint is configured, the LLM gets both transcripts,
   the spans they disagree on, your dictionary, the app, and examples of how you have wanted
   earlier dictations to read. It returns a *literal* guess and a *polished* guess.

The LLM never hears the audio. That is why the second pass exists: it is the only other
thing that listened, and the literal guess has to stay close to one of the two transcripts
or it is thrown away. A polished guess that drifts too far from the transcript reads like
an answer rather than a cleanup and is thrown away too.

A dictation where every opinion agrees with what was pasted is marked as not needing
review, and sorts to the back of the queue. Guesses are never training data until you
confirm them.

### Using Claude without an API key

The LLM part speaks the Anthropic Messages API, so it works with an API key or with
[yagami](https://github.com/justin06lee/yagami), which serves a signed-in Claude Code over the
same API. Run yagami on the server box and point the teacher at it; the key is read from
yagami's own config, so nothing needs copying:

```toml
[teacher]
enabled = true
llm_base_url = "http://127.0.0.1:8787"   # yagami
llm_model = "claude-haiku-4-5"
```

If the LLM is unreachable or signed out, the second listen still runs and review still
marks the disputed words; the LLM part is retried with a growing backoff (up to an hour)
and fills in the guesses once it answers.

Only transcript text reaches the LLM — never audio.

### Settings

| Setting | Default | What it does |
|---|---|---|
| `teacher.enabled` | `false` | Master switch. Off, una behaves exactly as without it. |
| `teacher.second_asr` | `true` | Run the second listen. |
| `teacher.second_asr_model` | `large-v3` | Any faster-whisper model. |
| `teacher.second_asr_device` | `cpu` | `cuda` if the card has room next to serving. |
| `teacher.second_asr_compute_type` | `int8` | |
| `teacher.second_asr_cpu_threads` | `4` | Keeps the box responsive while it works. |
| `teacher.second_asr_beam_size` | `5` | |
| `teacher.llm_base_url` | `""` | Messages API endpoint; empty skips the LLM part. |
| `teacher.llm_api_key` | `""` | Else `ANTHROPIC_API_KEY`, else yagami's key. |
| `teacher.llm_model` | `claude-haiku-4-5` | |
| `teacher.backfill` | `true` | Also label dictations from before it was on. |

`GET /v1/teacher` reports how many dictations are waiting and the last LLM error.
`GET /v1/review/queue` returns unreviewed dictations with their labels, flagged ones first.
