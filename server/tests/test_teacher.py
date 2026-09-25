"""Teacher tests: alignment, guess parsing, and labeling with a fake second pass and LLM."""

import pytest
from conftest import wav_bytes

from una_server.config import CleanupConfig, TeacherConfig
from una_server.services.llm import LlmError, extract_json
from una_server.services.teacher import (
    Teacher,
    Word,
    build_label_prompt,
    disagreements,
    needs_review,
    parse_label,
)


def words(text: str, step: float = 0.5) -> list[Word]:
    return [Word(f" {w}", i * step, i * step + 0.4) for i, w in enumerate(text.split())]


# -- alignment ------------------------------------------------------------------


def test_a_misheard_word_is_one_span_with_its_audio_time():
    raw = "I want to use LAMA 3.2 for this."
    spans = disagreements(raw, words("I want to use Llama 3.2 for this."))
    assert len(spans) == 1
    span = spans[0]
    assert raw[span.start:span.end] == "LAMA"
    assert span.alt == "Llama"
    assert (span.t0, span.t1) == (2.0, 2.4)


def test_casing_punctuation_and_hyphens_are_not_disagreement():
    assert disagreements("Hello, world. Fine-tuning!", words("hello world fine tuning")) == []


def test_a_word_only_the_second_pass_heard_is_a_zero_width_span():
    raw = "send it Friday"
    (span,) = disagreements(raw, words("send it on Friday"))
    assert span.start == span.end == raw.index("Friday")
    assert span.alt == "on"


def test_a_word_only_the_first_pass_heard_has_an_empty_alt():
    raw = "send it on Friday"
    (span,) = disagreements(raw, words("send it Friday"))
    assert raw[span.start:span.end] == "on"
    assert span.alt == ""
    assert span.t0 is not None and span.t1 is not None


def test_nothing_heard_by_the_second_pass_disputes_everything():
    (span,) = disagreements("hello there", [])
    assert (span.start, span.end, span.alt) == (0, len("hello there"), "")


# -- when to ask ------------------------------------------------------------------


def test_needs_review_only_when_something_disagrees():
    common = dict(raw="um hello there", pasted="Hello there.")
    assert not needs_review(**common, spans=[], literal_guess=None, polished_guess=None)
    assert not needs_review(
        **common, spans=[], literal_guess="Um, hello there.", polished_guess="Hello there."
    )
    assert needs_review(**common, spans=[], literal_guess="um hello their", polished_guess=None)
    # style lives in casing and punctuation, so those count for the polished text
    assert needs_review(**common, spans=[], literal_guess=None, polished_guess="hello there")


# -- the LLM's reply ----------------------------------------------------------------


def test_extract_json_finds_the_object_among_prose_and_fences():
    reply = 'Sure!\n```json\n{"literal": "a", "polished": "b"}\n```'
    assert extract_json(reply) == {"literal": "a", "polished": "b"}
    with pytest.raises(LlmError):
        extract_json("no json here")


def test_parse_label_keeps_close_guesses():
    literal, polished = parse_label(
        {"literal": "Um, I want to use Llama 3.2.", "polished": "i want to use llama 3.2"},
        raw="Um, I want to use LAMA 3.2.",
        second="Um, I want to use Llama 3.2.",
    )
    assert literal == "Um, I want to use Llama 3.2."
    assert polished == "i want to use llama 3.2"


def test_parse_label_drops_invention_and_replies():
    literal, polished = parse_label(
        {
            "literal": "Something neither recognizer heard at all, frankly.",
            "polished": "The OnePlus 2 was released in July 2016.",
        },
        raw="Tell me the answer to OnePlus 2?",
        second="Tell me the answer to OnePlus two?",
    )
    assert literal is None
    assert polished is None


def test_parse_label_rejects_non_objects():
    with pytest.raises(LlmError):
        parse_label(["literal"], raw="x", second=None)


def test_label_prompt_carries_both_transcripts_and_the_dispute():
    raw = "use LAMA please"
    spans = disagreements(raw, words("use Llama please"))
    prompt = build_label_prompt(
        raw=raw, second="use Llama please", spans=spans, app_name="Alacritty",
        tone="neutral", phrases=["Llama"], examples=[],
    )
    assert "Transcript A (fast model; what was pasted): use LAMA please" in prompt
    assert "Transcript B" in prompt
    assert '"LAMA" vs "Llama"' in prompt
    assert "Llama" in prompt.split("Dictionary")[1]


# -- labeling end to end, with fakes -------------------------------------------------


class FakeSecond:
    def __init__(self, text: str):
        self.text = text

    async def transcribe(self, samples, initial_prompt):
        return words(self.text)


class FakeLlm:
    def __init__(self, reply=None, error: str | None = None):
        self.reply = reply
        self.error = error
        self.calls = 0

    async def json_reply(self, system, user):
        self.calls += 1
        if self.error:
            raise LlmError(self.error)
        return self.reply

    async def aclose(self):
        pass


def _teacher(state, *, second: str, llm: FakeLlm | None) -> Teacher:
    cfg = TeacherConfig(enabled=True, llm_base_url="")
    teacher = Teacher(cfg, CleanupConfig(), state.db, "en")
    teacher.second = FakeSecond(second)
    teacher.llm = llm
    state.teacher = teacher
    return teacher


def _dictate(client) -> str:
    resp = client.post(
        "/v1/dictations",
        files={"audio": ("u.wav", wav_bytes(), "audio/wav")},
        data={"app_name": "Alacritty"},
    )
    return resp.json()["id"]


async def test_labels_prefill_review_and_order_the_queue(client):
    state = client.app.state.una
    agreed = _dictate(client)  # FakeTranscriber: "um so this is a test dictation"
    flagged = _dictate(client)

    teacher = _teacher(state, second="um so this is the test dictation", llm=None)
    assert await teacher.tick()  # newest first: `flagged`
    teacher.second = FakeSecond("um so this is a test dictation")
    assert await teacher.tick()  # then `agreed`
    assert not await teacher.tick()

    detail = client.get(f"/v1/dictations/{flagged}").json()
    label = detail["teacher"]
    assert label["status"] == "done"
    assert label["second_text"] == "um so this is the test dictation"
    assert label["needs_review"] is True
    (span,) = label["disagreements"]
    assert detail["raw_text"][span["start"]:span["end"]] == "a"
    assert span["alt"] == "the"

    assert client.get(f"/v1/dictations/{agreed}").json()["teacher"]["needs_review"] is False

    queue = client.get("/v1/review/queue").json()
    assert [item["id"] for item in queue["items"]] == [flagged, agreed]
    assert queue["pending"] == 2
    assert queue["needs_review"] == 1

    # reviewing takes it out of the queue
    client.put(f"/v1/dictations/{flagged}/correction", json={"action": "accepted"})
    queue = client.get("/v1/review/queue").json()
    assert [item["id"] for item in queue["items"]] == [agreed]


async def test_llm_guesses_are_stored_and_failures_back_off(client):
    state = client.app.state.una
    dictation = _dictate(client)
    llm = FakeLlm(error="not authorized: OAuth session expired")
    teacher = _teacher(state, second="um so this is a test dictation", llm=llm)

    assert await teacher.tick()
    label = client.get(f"/v1/dictations/{dictation}").json()["teacher"]
    assert label["status"] == "partial"
    assert "OAuth" in label["llm_error"]
    assert teacher.last_error is not None
    # backed off: the partial label is not retried straight away
    assert not await teacher.tick()
    assert llm.calls == 1

    # once the backoff lapses and the LLM answers, the partial label is finished
    teacher._llm_blocked_until = 0.0
    llm.error = None
    llm.reply = {"literal": "Um, so this is a test dictation.", "polished": "so this is a test dictation"}
    assert await teacher.tick()
    label = client.get(f"/v1/dictations/{dictation}").json()["teacher"]
    assert label["status"] == "done"
    assert label["literal_guess"] == "Um, so this is a test dictation."
    assert label["polished_guess"] == "so this is a test dictation"
    assert label["llm_error"] is None
    assert label["needs_review"] is True  # the polish differs from what was pasted


async def test_a_failed_second_pass_is_recorded_not_retried(client):
    state = client.app.state.una
    dictation = _dictate(client)
    teacher = _teacher(state, second="", llm=None)

    class Broken:
        async def transcribe(self, samples, initial_prompt):
            raise RuntimeError("model download failed")

    teacher.second = Broken()
    assert await teacher.tick()
    label = client.get(f"/v1/dictations/{dictation}").json()["teacher"]
    assert label["status"] == "failed"
    assert "download" in label["error"]
    assert not await teacher.tick()


def test_teacher_status_endpoint(client):
    assert client.get("/v1/teacher").json() == {
        "enabled": False, "second_asr_model": None, "llm_model": None,
        "unlabeled": 0, "partial": 0, "last_error": None,
    }


async def test_examples_prefer_confirmed_pairs_from_the_same_app(client):
    state = client.app.state.una
    first = _dictate(client)
    client.put(
        f"/v1/dictations/{first}/correction",
        json={"action": "accepted", "polished_text": "so this is a test"},
    )
    second = _dictate(client)
    teacher = _teacher(state, second="x", llm=None)
    examples = await teacher.examples(second, "Alacritty")
    assert [(e.said, e.wrote) for e in examples] == [
        ("um so this is a test dictation", "so this is a test")
    ]


async def test_a_reply_pasted_as_cleanup_is_flagged(client):
    state = client.app.state.una
    dictation = _dictate(client)
    await state.db.execute(
        "UPDATE dictations SET cleaned_text = ?, cleanup_applied = 1 WHERE id = ?",
        ("The OnePlus 2 was released in July 2016.", dictation),
    )
    await state.db.commit()
    assert client.get(f"/v1/dictations/{dictation}").json()["cleanup_diverged"] is True
    await state.db.execute(
        "UPDATE dictations SET cleaned_text = ? WHERE id = ?",
        ("so this is a test dictation", dictation),
    )
    await state.db.commit()
    assert client.get(f"/v1/dictations/{dictation}").json()["cleanup_diverged"] is False
