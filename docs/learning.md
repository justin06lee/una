# Learning from your edits

una's two fine-tunes — Whisper on your voice, the cleanup LLM on your phrasing — are only as
good as the training pairs they get. A pair is a dictation plus the right answer for it, and
the most reliable source of right answers is what you do to the text right after it lands.

The client watches for that and files it automatically. Settings → **Learning**.

## What gets captured

Every paste ends in exactly one of these:

| What you do | What is filed | Where the pair goes |
|---|---|---|
| Nothing — you keep working | `accepted` | Confirms the transcript. ASR pair only. |
| Fix a word or two | `polished` | Your text becomes the style (cleanup-LLM) target. The transcript stays in Review. |
| Delete the whole thing | `excluded` | Marks the utterance as not worth training on. |
| Edit text una *didn't* paste | nothing | Not a correction; ignored. |

An edit never becomes the ASR target. What una pasted was the *cleaned* text, so your fix
says how it should have read, not what you literally said — train Whisper on it and it learns
to drop your fillers and self-corrections instead of hearing them. The transcript is judged
separately, in Review.

Untouched dictations are the bulk of it, and they are the reason this matters: corrections
alone are a small, self-selecting sample of only the model's mistakes. Turn that off with
**Count untouched dictations as correct** if you would rather only teach it from real fixes.

The server applies its usual eligibility filter to whatever arrives — an "edit" whose
normalized edit distance from the raw transcript exceeds `training.max_edit_distance` (0.30)
is stored but marked ineligible, because rewriting the content of a sentence would teach the
model to paraphrase rather than to hear.

Accepting is treated carefully for a related reason. What you approved by not touching it is
the *cleaned* text, but the ASR pair targets the raw transcript, which you never saw — and
cleanup can quietly repair a mishearing, especially when a dictionary hint points at it.
So when cleanup rewrote more than `max_edit_distance` of the transcript, the accept is
recorded but held back from training and left in Review to be judged by hand. Corrections
typed in the dashboard are exempt: there the raw transcript is on screen, and it is what you
are approving.

An accepted dictation contributes an ASR pair only, never a style pair — the text being
accepted is the cleanup model's own output, and training it on that would teach it nothing
while drowning out the pairs where you really did change the wording.

## Two ways it watches, depending on the app

**Silently, via the accessibility API.** After a paste, una snapshots the focused field's text
and where in it the paste landed. When you stop typing (`settle_ms`, default 1.2 s), it reads
the field again and diffs the two to recover what the pasted span became. Nothing appears on
screen. This works in native AppKit fields, Safari, Mail, Notes, and most Electron apps.

**With the correction window, everywhere else.** GPU-rendered terminals (Ghostty, Alacritty,
kitty), canvas editors, and password fields don't publish their text to the accessibility API,
so there is nothing to diff. There, the first editing keystroke opens a small window holding
what una pasted. Fix it, press ⌘↩, and the corrected text is both filed and written back into
the app you were in. Escape walks away and records nothing.

Turn the window off with **Ask when the text can't be read** — edits in those apps then simply
aren't captured (untouched pastes still are).

## Fixing one after the fact

The watch window is short by design, and the silent path deliberately gives up
whenever it can't attribute an edit. When you notice a bad transcription later — or
you were in an app where nothing was captured — the tray's **Fix Last Dictation…**
opens the correction window for the most recent paste, however long ago it was. That
files the pair; it doesn't try to rewrite anything in the app, because by then the
caret is long gone.

## What it will not do

The design errs toward missing a pair rather than inventing one, because a wrong training pair
is worse than a missing one. Nothing is filed when:

- The change falls outside the text una pasted — you were editing your own prose.
- The edit spans the boundary of the pasted text, so it can't be separated from your own.
- Focus left the field before you stopped typing.
- You dictated again before the first watch finished; the field now holds both, and neither
  can be attributed cleanly.
- The paste didn't land where the caret said it would (an app that rewrites text on the way
  in — smart quotes, autocorrect). The correction window is used instead.

In the correction window, writing the text back is also skipped — while still filing the pair —
when you moved the caret mid-edit, since una then no longer knows how much text to replace.
Deleting the wrong number of characters in your terminal is a worse outcome than not pasting.

## Privacy

Nothing new leaves your machine. The audio and both transcripts already go to your server for
every dictation; this adds only what you changed afterwards, attached to the dictation it
belongs to. Keystrokes are classified (insert / delete / navigate) and counted — never their
contents, never stored, never sent — and only during the watch window after a paste.

Turning **Learn from my edits** off stops all of it; the dashboard's Review page still works.

## Settings

| Setting | Default | What it does |
|---|---|---|
| Learn from my edits | on | Master switch. |
| Count untouched dictations as correct | on | Files `accepted` when nothing is typed. |
| Ask when the text can't be read | on | The correction window in terminals etc. |
| Watch for edits | 25 s | How long after a paste edits still count. |
| Edit is finished after | 1200 ms | Typing silence that ends an edit. |

In `config.toml`:

```toml
[correction]
enabled = true
watch_seconds = 25
settle_ms = 1200
auto_accept = true
popup = true
```

## Platform support

macOS only for now. The silent path needs the accessibility text API and the keystroke signal
needs the event tap — both of which una already uses, under the Accessibility permission it
already requires for pasting. No new permission is requested.

Linux has no equivalent that works across X11, Wayland, GTK and Qt alike, so the whole feature
is inert there and corrections come from the dashboard's Review page as before.

## Seeing what it collected

The dashboard's Review page shows every dictation with its correction; pairs carry a `source`
of `auto` (captured silently), `popup` (the correction window) or `review` (typed in the
dashboard). The Training page counts eligible minutes and is where a fine-tune is launched —
or turn on auto-train and it launches itself once enough new pairs accumulate.
