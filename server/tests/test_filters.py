from una_server.api.dictations import _guard_unseen_transcript
from una_server.schemas import CorrectionRequest
from una_server.training.filters import check_pair, norm_edit_distance, normalize_text


def test_normalize_strips_case_punct_whitespace():
    assert normalize_text("Hello,   World!") == "hello world"
    assert normalize_text("don't  stop") == "don't stop"
    assert normalize_text("") == ""


def test_edit_distance_identical_after_normalization():
    assert norm_edit_distance("Hello, world!", "hello world") == 0.0


def test_edit_distance_full_rewrite_is_high():
    assert norm_edit_distance("send the report tomorrow", "completely different text here") > 0.5


def _pair(**overrides):
    base = dict(
        action="edited",
        raw_text="send the report on monday",
        corrected_text="send the report on Monday",
        duration_ms=5000,
        max_edit_distance=0.30,
    )
    base.update(overrides)
    return check_pair(**base)


def test_accepted_is_gold():
    result = _pair(action="accepted", corrected_text=None)
    assert result.eligible and result.distance == 0.0


def test_small_fix_eligible():
    result = _pair(corrected_text="send the report on tuesday")
    assert result.eligible
    assert result.distance is not None and result.distance <= 0.30


def test_content_rewrite_excluded():
    result = _pair(corrected_text="actually let's schedule a meeting for next week instead")
    assert not result.eligible
    assert "content rewrite" in result.reason


def test_too_short_and_too_long_excluded():
    assert not _pair(duration_ms=500).eligible
    assert not _pair(duration_ms=31_000).eligible


def test_skipped_and_excluded_actions():
    assert not _pair(action="skipped").eligible
    assert not _pair(action="excluded").eligible


def test_empty_correction_excluded():
    assert not _pair(corrected_text="  !!! ").eligible


def test_boundary_durations_eligible():
    assert _pair(duration_ms=1000).eligible
    assert _pair(duration_ms=30_000).eligible


# ---------------------------------------------------------------------------
# Auto-accept guard: a client-captured "accepted" only vouches for the text the
# user actually saw, which with cleanup on is not the raw transcript.
# ---------------------------------------------------------------------------


def _row(raw, cleaned, applied=True):
    return {"raw_text": raw, "cleaned_text": cleaned, "cleanup_applied": applied}


def _accept(source="auto", **row):
    return _guard_unseen_transcript(
        check_pair(
            action="accepted",
            raw_text=row["raw_text"],
            corrected_text=row["raw_text"],
            duration_ms=2000,
            max_edit_distance=0.3,
        ),
        CorrectionRequest(action="accepted", source=source),
        row,
        0.3,
    )


def test_auto_accept_survives_light_cleanup():
    """Punctuation and casing don't hide mishearings, so the approval carries."""
    result = _accept(**_row("um so we should deploy", "So we should deploy."))
    assert result.eligible


def test_auto_accept_blocked_when_cleanup_rewrote_the_text():
    result = _accept(**_row("we should deploy the cluster", "Ship it."))
    assert not result.eligible
    assert "never shown" in result.reason


def test_auto_accept_unaffected_when_no_cleanup_ran():
    row = _row("we should deploy the cluster", None, applied=False)
    assert _accept(**row).eligible


def test_hand_review_accept_is_never_second_guessed():
    """The Review page shows the raw transcript, so that is what was approved."""
    result = _accept(source="review", **_row("we should deploy the cluster", "Ship it."))
    assert result.eligible
