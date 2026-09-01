from conftest import wav_bytes


def post_dictation(client, **form):
    return client.post(
        "/v1/dictations",
        files={"audio": ("utterance.wav", wav_bytes(), "audio/wav")},
        data={"app_name": "Slack", **form},
    )


def test_dictation_money_path(client):
    resp = post_dictation(client)
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert body["text"] == "um so this is a test dictation"
    assert body["cleanup_applied"] is False  # cleanup disabled in test config
    assert body["duration_ms"] == 2000
    assert body["asr_model"] == "base:large-v3-turbo"

    listing = client.get("/v1/dictations").json()
    assert len(listing["items"]) == 1
    assert listing["items"][0]["reviewed"] is False

    detail = client.get(f"/v1/dictations/{body['id']}").json()
    assert detail["raw_text"] == body["raw_text"]

    audio = client.get(f"/v1/dictations/{body['id']}/audio")
    assert audio.status_code == 200
    assert audio.headers["content-type"].startswith("audio/wav")


def test_utterance_id_dedupes_retries(client):
    first = post_dictation(client, utterance_id="abc-123").json()
    second = post_dictation(client, utterance_id="abc-123").json()
    assert first["id"] == second["id"]
    assert len(client.get("/v1/dictations").json()["items"]) == 1


def test_correction_source_defaults_and_round_trips(client):
    """The client tags auto-captured edits so the dashboard can tell them from
    hand review; omitting the field keeps the historical 'review' value."""
    dictation_id = post_dictation(client).json()["id"]

    default = client.put(
        f"/v1/dictations/{dictation_id}/correction", json={"action": "accepted"}
    ).json()
    assert default["source"] == "review"

    auto = client.put(
        f"/v1/dictations/{dictation_id}/correction",
        json={
            "action": "edited",
            "corrected_text": "um so this is a test dictation okay",
            "source": "auto",
        },
    ).json()
    assert auto["source"] == "auto"
    assert auto["training_eligible"] is True

    detail = client.get(f"/v1/dictations/{dictation_id}").json()
    assert detail["correction_source"] == "auto"


def test_correction_flow(client):
    dictation_id = post_dictation(client).json()["id"]

    accepted = client.put(
        f"/v1/dictations/{dictation_id}/correction", json={"action": "accepted"}
    ).json()
    assert accepted["training_eligible"] is True
    assert accepted["norm_edit_distance"] == 0.0

    rewrite = client.put(
        f"/v1/dictations/{dictation_id}/correction",
        json={"action": "edited", "corrected_text": "entirely unrelated replacement sentence"},
    ).json()
    assert rewrite["training_eligible"] is False
    assert "content rewrite" in rewrite["eligibility_reason"]

    small_fix = client.put(
        f"/v1/dictations/{dictation_id}/correction",
        json={"action": "edited", "corrected_text": "um so this is a test dictation okay"},
    ).json()
    assert small_fix["training_eligible"] is True


def test_polished_text_style_pair(client):
    dictation_id = post_dictation(client).json()["id"]

    saved = client.put(
        f"/v1/dictations/{dictation_id}/correction",
        json={"action": "accepted", "polished_text": "This is a test dictation."},
    ).json()
    assert saved["polished_text"] == "This is a test dictation."

    detail = client.get(f"/v1/dictations/{dictation_id}").json()
    assert detail["polished_text"] == "This is a test dictation."
    assert client.get("/v1/training/eligibility").json()["style_pairs"] == 1

    # clearing the polish removes the style pair
    # a skip that omits the field must NOT wipe the stored style pair
    skipped = client.put(
        f"/v1/dictations/{dictation_id}/correction", json={"action": "skipped"}
    ).json()
    assert skipped["polished_text"] == "This is a test dictation."
    assert client.get("/v1/training/eligibility").json()["style_pairs"] == 1

    cleared = client.put(
        f"/v1/dictations/{dictation_id}/correction",
        json={"action": "accepted", "polished_text": "   "},
    ).json()
    assert cleared["polished_text"] is None
    assert client.get("/v1/training/eligibility").json()["style_pairs"] == 0


def test_review_queue(client):
    first = post_dictation(client).json()["id"]
    second = post_dictation(client).json()["id"]

    queue_head = client.get("/v1/review/next").json()
    assert queue_head["id"] == first

    client.put(f"/v1/dictations/{first}/correction", json={"action": "accepted"})
    assert client.get("/v1/review/next").json()["id"] == second

    client.put(f"/v1/dictations/{second}/correction", json={"action": "excluded"})
    assert client.get("/v1/review/next").json() is None


def test_dictionary_biases_initial_prompt(client):
    entry = client.post("/v1/dictionary", json={"phrase": "Anthropic"}).json()
    assert entry["phrase"] == "Anthropic"

    post_dictation(client)
    fake = client.app.state.una.models.transcriber
    assert "Anthropic" in fake.last_initial_prompt

    dup = client.post("/v1/dictionary", json={"phrase": "Anthropic"})
    assert dup.status_code == 400

    assert client.delete(f"/v1/dictionary/{entry['id']}").status_code == 204


def test_delete_dictation_removes_audio(client):
    body = post_dictation(client).json()
    assert client.delete(f"/v1/dictations/{body['id']}").status_code == 204
    assert client.get(f"/v1/dictations/{body['id']}").status_code == 404
    assert client.get("/v1/dictations").json()["items"] == []


def test_settings_roundtrip(client):
    updated = client.put("/v1/settings", json={"training.threshold_minutes": 10.0}).json()
    assert updated["training.threshold_minutes"] == 10.0
    assert client.put("/v1/settings", json={"nope": 1}).status_code == 400


def test_eligibility_counts(client):
    dictation_id = post_dictation(client).json()["id"]
    client.put(f"/v1/dictations/{dictation_id}/correction", json={"action": "accepted"})
    eligibility = client.get("/v1/training/eligibility").json()
    # the sample may land in the eval holdout; either way the endpoint answers coherently
    assert eligibility["eligible_pairs"] in (0, 1)
    assert eligibility["ready"] is False


def test_health(client):
    body = client.get("/v1/health").json()
    assert body["asr_model_loaded"] is True
    assert body["ollama"] == "unreachable" or body["ollama"] == "ok"


async def test_sounds_like_feeds_cleanup_not_whisper(client):
    state = client.app.state.una
    client.post("/v1/dictionary", json={"phrase": "Tauri", "sounds_like": "towery"})
    client.post("/v1/dictionary", json={"phrase": "soxr"})
    from una_server.api.dictations import _active_phrases

    phrases, hinted = await _active_phrases(state)
    assert phrases == ["Tauri", "soxr"]
    assert hinted == ['Tauri (often misheard as "towery")', "soxr"]


async def test_client_field_is_stored(client):
    resp = client.post(
        "/v1/dictations",
        files={"audio": ("u.wav", wav_bytes(), "audio/wav")},
        data={"clean": "false", "client": "una-desktop/0.1.0 test"},
    )
    did = resp.json()["id"]
    state = client.app.state.una
    async with state.db.execute("SELECT client FROM dictations WHERE id = ?", (did,)) as cur:
        row = await cur.fetchone()
    assert row["client"] == "una-desktop/0.1.0 test"


def test_settings_reject_bool_for_float(client):
    resp = client.put("/v1/settings", json={"cleanup.timeout_s": True})
    assert resp.status_code == 400
