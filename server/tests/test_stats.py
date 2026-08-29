from datetime import date

from conftest import wav_bytes

from una_server.api.system import _streaks


def post_dictation(client, **form):
    return client.post(
        "/v1/dictations",
        files={"audio": ("utterance.wav", wav_bytes(), "audio/wav")},
        data={"app_name": "Slack", **form},
    )


# ---------------------------------------------------------------- streaks


def test_streak_empty():
    streak = _streaks([], date(2026, 8, 28))
    assert (streak.current, streak.longest) == (0, 0)


def test_streak_counts_consecutive_days_ending_today():
    days = ["2026-08-26", "2026-08-27", "2026-08-28"]
    streak = _streaks(days, date(2026, 8, 28))
    assert (streak.current, streak.longest) == (3, 3)


def test_streak_tolerates_not_having_dictated_yet_today():
    # Yesterday still counts: the day isn't over.
    streak = _streaks(["2026-08-26", "2026-08-27"], date(2026, 8, 28))
    assert streak.current == 2


def test_streak_breaks_after_a_missed_day():
    streak = _streaks(["2026-08-20", "2026-08-21"], date(2026, 8, 28))
    assert streak.current == 0
    assert streak.longest == 2


def test_longest_streak_survives_a_later_gap():
    days = ["2026-08-01", "2026-08-02", "2026-08-03", "2026-08-10", "2026-08-28"]
    streak = _streaks(days, date(2026, 8, 28))
    assert streak.longest == 3
    assert streak.current == 1


def test_streak_ignores_duplicate_days():
    streak = _streaks(["2026-08-27", "2026-08-27", "2026-08-28"], date(2026, 8, 28))
    assert (streak.current, streak.longest) == (2, 2)


# ---------------------------------------------------------------- endpoint


def test_stats_shape_on_empty_db(client):
    stats = client.get("/v1/stats").json()
    assert stats["totals"] == {
        "dictations": 0,
        "words": 0,
        "ms": 0,
        "avg_wpm": 0.0,
        "days_active": 0,
    }
    assert stats["streak"] == {"current": 0, "longest": 0}
    assert stats["per_day"] == []
    assert stats["by_app"] == []
    assert stats["review"] == {"backlog": 0, "reviewed": 0, "eligible": 0}


def test_stats_counts_words_and_wpm(client):
    # "um so this is a test dictation" is 7 words; each clip is 2s, so 3 clips
    # give 21 words over 0.1 minutes of audio.
    for _ in range(3):
        post_dictation(client)
    stats = client.get("/v1/stats").json()

    assert stats["totals"]["dictations"] == 3
    assert stats["totals"]["words"] == 21
    assert stats["totals"]["ms"] == 6000
    assert stats["totals"]["avg_wpm"] == 210.0
    assert stats["totals"]["days_active"] == 1


def test_stats_groups_by_app_and_tracks_review(client):
    post_dictation(client, app_name="Slack")
    post_dictation(client, app_name="Mail")
    post_dictation(client, app_name="Mail")
    # An empty app name is bucketed rather than dropped.
    post_dictation(client, app_name="")

    stats = client.get("/v1/stats").json()
    by_app = {row["app"]: row["n"] for row in stats["by_app"]}
    assert by_app == {"Mail": 2, "Slack": 1, "Unknown": 1}
    assert stats["review"]["backlog"] == 4
    assert stats["review"]["reviewed"] == 0

    first = client.get("/v1/dictations").json()["items"][0]["id"]
    client.put(f"/v1/dictations/{first}/correction", json={"action": "accepted"})
    stats = client.get("/v1/stats").json()
    assert stats["review"] == {"backlog": 3, "reviewed": 1, "eligible": 1}


def test_stats_deleted_dictations_are_excluded(client):
    keep = post_dictation(client).json()["id"]
    drop = post_dictation(client).json()["id"]
    client.delete(f"/v1/dictations/{drop}")

    stats = client.get("/v1/stats").json()
    assert stats["totals"]["dictations"] == 1
    assert stats["review"]["backlog"] == 1
    assert keep  # the surviving row is the one counted


def test_stats_dictionary_hits_are_reported(client):
    client.post("/v1/dictionary", json={"phrase": "dictation"})
    post_dictation(client)  # transcript contains "dictation" -> one hit
    stats = client.get("/v1/stats").json()
    assert stats["cleanup"]["dictionary_hits"] == 1
    # Cleanup is disabled in the test config, so nothing was stripped.
    assert stats["cleanup"] == {"applied": 0, "words_removed": 0, "dictionary_hits": 1}
