"""The writing corpus: harvesting, back-translation parsing, the API, and the dataset it feeds.

None of this needs torch or a live LLM.
"""

import json
import sqlite3

from una_server.config import CleanupConfig, TeacherConfig
from una_server.db import MIGRATIONS_DIR
from una_server.services.teacher import Teacher
from una_server.tools import writing
from una_server.training import backtranslate as bt
from una_server.training.style_dataset import build_style_datasets
from una_server.training.style_promote import probe_gate
from una_server.training.style_runner import _choose_eval

TS = "2026-09-25T00:00:00+00:00"

# -- chunking and hashing ------------------------------------------------------------


def test_chunk_keeps_short_text_whole_and_splits_long_text_at_sentences():
    assert bt.chunk("ok so is it done type shi") == ["ok so is it done type shi"]
    long = " ".join(f"Sentence number {i} goes right here." for i in range(40))
    pieces = bt.chunk(long, max_words=30)
    assert all(len(p.split()) <= 30 for p in pieces)
    assert " ".join(pieces) == long
    assert all(p.endswith(".") for p in pieces)


def test_chunk_splits_paragraphs_and_hard_wraps_enormous_sentences():
    assert bt.chunk("first para\n\nsecond para") == ["first para", "second para"]
    pieces = bt.chunk(" ".join(["word"] * 25), max_words=10)
    assert [len(p.split()) for p in pieces] == [10, 10, 5]


def test_content_hash_ignores_whitespace_only():
    assert bt.content_hash("hello  there\n") == bt.content_hash("hello there")
    assert bt.content_hash("hello there") != bt.content_hash("Hello there")


# -- parsing the LLM's back-translations ------------------------------------------------


def test_parse_reply_keeps_good_pairs_and_repairs_or_drops_bad_ones():
    items = [
        bt.Item("a", "ok so is it done type shi its pushed and all?"),
        bt.Item("b", "fix the proejcts page"),
        bt.Item("c", "lowk this is fine"),
        bt.Item("d", "make it faster"),
    ]
    reply = [
        {"id": "a", "target": "ok so is it done type shi its pushed and all?",
         "spoken": "Okay, so is it done, type shit? It's pushed and all?"},
        {"id": "b", "target": "fix the projects page", "spoken": "Fix the, uh, projects page."},
        # "fixed" deliberate lowercase: the original is kept as the target
        {"id": "c", "target": "Lowkey, this is fine.", "spoken": "Lowkey this is fine."},
        # a spoken version unrelated to the text is dropped
        {"id": "d", "target": "make it faster", "spoken": "What a lovely day it is outside today."},
        {"id": "zzz", "target": "x", "spoken": "x"},
        "not an object",
    ]
    pairs = {p.id: p for p in bt.parse_reply(reply, items)}
    assert set(pairs) == {"a", "b", "c"}
    assert pairs["b"].target == "fix the projects page"
    assert pairs["c"].target == "lowk this is fine"
    assert bt.parse_reply({"not": "a list"}, items) == []


def test_build_prompt_carries_calibration_and_items():
    prompt = bt.build_prompt([bt.Item("x1", "hello there")], ["Um, so, hello."])
    assert "Um, so, hello." in prompt
    assert json.loads(prompt.split("Items:\n")[1]) == [{"id": "x1", "text": "hello there"}]


# -- harvesting --------------------------------------------------------------------


def test_looks_typed_keeps_prose_and_drops_pastes_secrets_and_harness_prompts():
    assert writing.looks_typed("wait so like can we use tailscale ssh for this or wat")
    for text in [
        "<command-name>/clear</command-name>",
        "/review 42",
        "here is the error ```Traceback (most recent call last)```",
        "use this key sk-ant-api03-abcdefghijklmnopqrstuvwxyz",
        "Reply with the single word ok.",
        "Write a haiku about each of the twelve months.",
        "\n".join(f"- item {i} of a pasted list" for i in range(6)),
        "## Leads from the sweep agents\nsomething",
        "{\"GOAL\": \"json from a tool\"}",
        "ok",
    ]:
        assert not writing.looks_typed(text), text


def _write_claude_code(root, messages):
    project = root / "proj"
    project.mkdir(parents=True)
    lines = []
    for message in messages:
        lines.append(json.dumps(message))
    (project / "s.jsonl").write_text("\n".join(lines) + "\n")


def test_harvest_reads_user_turns_and_drops_repeats(tmp_path, monkeypatch):
    claude_root = tmp_path / "claude"
    typed = "yo can you make the dashboard look way cleaner, like less stuff on the home page"
    repeated = "this exact message is sent by a script over and over"
    _write_claude_code(claude_root, [
        {"type": "user", "message": {"role": "user", "content": typed}},
        {"type": "user", "message": {"role": "user", "content": [{"type": "text", "text": typed}]}},
        {"type": "user", "isMeta": True, "message": {"role": "user", "content": "meta stuff here ok"}},
        {"type": "user", "toolUseResult": {}, "message": {"role": "user", "content": "tool output text"}},
        {"type": "assistant", "message": {"role": "assistant", "content": "an assistant reply"}},
        *[{"type": "user", "message": {"role": "user", "content": repeated}} for _ in range(3)],
    ])
    monkeypatch.setitem(writing.SOURCES, "claude-code", (
        writing._claude_code, claude_root, "Claude Code"))
    samples = writing.harvest(["claude-code"])
    assert [(s.source, s.app_name, s.text) for s in samples] == [("claude-code", "Claude Code", typed)]


# -- the API -----------------------------------------------------------------------


def test_corpus_upload_is_idempotent_and_fills_in_later(client):
    item = {"source": "claude-code", "app_name": "Claude Code", "written_text": "make it faster pls"}
    first = client.post("/v1/style/corpus", json={"items": [item]}).json()
    assert first == {"added": 1, "updated": 0, "unchanged": 0}
    again = client.post("/v1/style/corpus", json={"items": [item]}).json()
    assert again == {"added": 0, "updated": 0, "unchanged": 1}

    key = bt.content_hash(item["written_text"])
    assert client.post("/v1/style/corpus/known", json={"hashes": [key]}).json() == {"finished": []}

    finished = {**item, "target_text": "make it faster pls", "spoken_text": "Make it faster, please.",
                "generator_model": "claude-haiku-4-5"}
    assert client.post("/v1/style/corpus", json={"items": [finished]}).json()["updated"] == 1
    assert client.post("/v1/style/corpus/known", json={"hashes": [key]}).json() == {"finished": [key]}
    # a finished sample is never overwritten
    other = {**finished, "spoken_text": "Something else."}
    assert client.post("/v1/style/corpus", json={"items": [other]}).json()["unchanged"] == 1

    stats = client.get("/v1/style/corpus/stats").json()
    assert stats["samples"] == 1 and stats["finished"] == 1

    eligibility = client.get("/v1/training/eligibility").json()
    assert eligibility["style_writing_pairs"] == 1
    assert eligibility["style_ready"] is False


def test_calibration_returns_real_transcripts(client):
    from conftest import wav_bytes

    client.app.state.una.models.transcriber.text = "um so this is a much longer test dictation okay"
    client.post("/v1/dictations", files={"audio": ("u.wav", wav_bytes(), "audio/wav")})
    got = client.get("/v1/style/calibration").json()["transcripts"]
    assert got == ["um so this is a much longer test dictation okay"]


# -- the dataset ---------------------------------------------------------------------


def _db() -> sqlite3.Connection:
    conn = sqlite3.connect(":memory:")
    conn.row_factory = sqlite3.Row
    for path in sorted(MIGRATIONS_DIR.glob("*.sql")):
        conn.executescript(path.read_text())
    return conn


def _dictation(conn, did, raw, *, cleaned=None, holdout=0, app="Alacritty"):
    conn.execute(
        """INSERT INTO dictations (id, created_at, app_name, audio_path, duration_ms, raw_text,
           cleaned_text, cleanup_applied, eval_holdout) VALUES (?, ?, ?, '/x.wav', 2000, ?, ?, ?, ?)""",
        (did, TS, app, raw, cleaned, 1 if cleaned else 0, holdout),
    )


def _correction(conn, did, polished, action="accepted"):
    conn.execute(
        """INSERT INTO corrections (id, dictation_id, action, polished_text, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?)""",
        (f"c-{did}", did, action, polished, TS, TS),
    )


def _writing(conn, wid, spoken, target, holdout=0):
    conn.execute(
        """INSERT INTO style_corpus (id, source, app_name, written_text, content_hash, target_text,
           spoken_text, eval_holdout, created_at, updated_at)
           VALUES (?, 'claude-code', 'Claude Code', ?, ?, ?, ?, ?, ?, ?)""",
        (wid, target, f"h-{wid}", target, spoken, holdout, TS, TS),
    )


def test_dataset_mixes_confirmed_writing_and_silver_with_preferences():
    conn = _db()
    _dictation(conn, "d1", "um fix the page", cleaned="Fix the page.")
    _correction(conn, "d1", "fix the page")  # differs from what was pasted -> a preference
    _dictation(conn, "d2", "same thing", cleaned="Same thing.")
    _correction(conn, "d2", "Same thing.")  # identical to what was pasted -> no preference
    _dictation(conn, "d3", "held out", holdout=1)
    _correction(conn, "d3", "held out.")
    _dictation(conn, "d4", "unconfirmed words")
    conn.execute(
        """INSERT INTO teacher_labels (dictation_id, status, polished_guess, created_at, updated_at)
           VALUES ('d4', 'done', 'unconfirmed words', ?, ?)""", (TS, TS))
    _writing(conn, "w1", "Make it faster, please.", "make it faster pls")
    _writing(conn, "w2", "Um, ship it.", "ship it", holdout=1)

    split = build_style_datasets(conn, gold_repeat=2)
    assert split.counts() == {"confirmed": 4, "writing": 1, "silver": 1}
    assert [s.polished_text for s in split.eval] == ["held out."]
    assert [s.polished_text for s in split.writing_eval] == ["ship it"]
    (pref,) = split.preferences
    assert (pref.chosen, pref.rejected, pref.origin) == ("fix the page", "Fix the page.", "edit")

    bare = build_style_datasets(conn, use_writing=False, use_silver=False)
    assert bare.counts() == {"confirmed": 2}
    assert bare.writing_eval == []


def test_eval_set_prefers_confirmed_then_writing():
    conn = _db()
    for i in range(3):
        _writing(conn, f"w{i}", f"Spoken {i}.", f"written {i}", holdout=1)
    split = build_style_datasets(conn)
    assert _choose_eval(split, 3)[0] == "writing"
    for i in range(3):
        _dictation(conn, f"d{i}", f"raw {i}", holdout=1)
        _correction(conn, f"d{i}", f"polished {i}")
    assert _choose_eval(build_style_datasets(conn), 3)[0] == "confirmed"
    try:
        _choose_eval(build_style_datasets(conn), 10)
    except RuntimeError as exc:
        assert "not enough held-out pairs" in str(exc)
    else:
        raise AssertionError("expected a RuntimeError")


def test_probe_gate_never_lets_a_candidate_answer_more():
    assert probe_gate(2, 0)[0]
    assert probe_gate(1, 1)[0]
    ok, reason = probe_gate(0, 1)
    assert not ok and "reply probes" in reason


# -- the teacher back-translates imported writing ----------------------------------------


class FakeLlm:
    def __init__(self, reply):
        self.reply = reply
        self.prompts = []

    async def json_reply(self, system, user):
        self.prompts.append(user)
        return self.reply

    async def aclose(self):
        pass


async def test_teacher_backtranslates_pending_writing(client):
    state = client.app.state.una
    items = [
        {"source": "claude-code", "app_name": "Claude Code", "written_text": "make it faster pls"},
        {"source": "claude-code", "app_name": "Claude Code", "written_text": "ship it"},
    ]
    client.post("/v1/style/corpus", json={"items": items})
    key = bt.content_hash("make it faster pls")[:12]
    teacher = Teacher(TeacherConfig(enabled=True, second_asr=False), CleanupConfig(), state.db, "en")
    teacher.llm = FakeLlm([{"id": key, "target": "make it faster pls", "spoken": "Make it faster, please."}])
    state.teacher = teacher

    assert await teacher.tick()
    stats = client.get("/v1/style/corpus/stats").json()
    assert stats == {"samples": 2, "finished": 1, "holdout": stats["holdout"]}
    # the one the LLM left out is marked, not retried forever
    assert not await teacher.tick()

    # finished writing now shows up as style examples for apps with the same tone
    examples = await teacher.examples("nope", "Alacritty")
    assert [e.wrote for e in examples if e.said is None] == ["make it faster pls"]
    assert await teacher.examples("nope", "Mail") == []
