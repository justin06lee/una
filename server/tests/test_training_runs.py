"""Run-launch API regression tests + auto-training trigger logic."""

import time

from conftest import wav_bytes

from una_server.services import autotrain


def test_start_run_does_not_conflict_with_its_own_row(client, fake_runner):
    # Regression: the queued row inserted by start_run must not trip the
    # active-run guard that jobs.start applies before spawning.
    resp = client.post("/v1/training/runs")
    assert resp.status_code == 201, resp.text
    run = resp.json()
    assert run["status"] == "queued"

    listed = client.get("/v1/training/runs").json()
    assert [r["id"] for r in listed] == [run["id"]]
    assert client.get(f"/v1/training/runs/{run['id']}").status_code == 200


def test_second_run_conflicts_and_first_survives(client, fake_runner):
    first = client.post("/v1/training/runs").json()
    resp = client.post("/v1/training/runs")
    assert resp.status_code == 409
    assert resp.json()["error"]["code"] == "RUN_ACTIVE"
    listed = client.get("/v1/training/runs").json()
    assert [r["id"] for r in listed] == [first["id"]]


def _make_eligible_pair(client, text="um so this is a test dictation"):
    resp = client.post(
        "/v1/dictations",
        files={"audio": ("u.wav", wav_bytes(), "audio/wav")},
        data={"clean": "false"},
    )
    d = resp.json()
    client.put(f"/v1/dictations/{d['id']}/correction", json={"action": "accepted"})
    return d["id"]


async def _force_out_of_holdout(state):
    await state.db.execute("UPDATE dictations SET eval_holdout = 0")
    await state.db.commit()


async def test_autotrain_conditions(client):
    state = client.app.state.una
    cfg = state.config.training
    launches = []

    async def fake_launch(st):
        launches.append(st)
        return {"id": "fake-run"}

    for _ in range(3):
        _make_eligible_pair(client)
    await _force_out_of_holdout(state)

    cfg.threshold_minutes = 0.01
    idle = time.monotonic() - cfg.auto_idle_minutes * 60 - 1

    # auto off -> never starts
    cfg.auto = False
    state.last_dictation_at = idle
    assert not await autotrain.maybe_start(state, launch=fake_launch)

    # on but recently active -> waits
    cfg.auto = True
    state.last_dictation_at = time.monotonic()
    assert not await autotrain.maybe_start(state, launch=fake_launch)

    # idle but below the data threshold -> waits
    state.last_dictation_at = idle
    cfg.threshold_minutes = 9999.0
    assert not await autotrain.maybe_start(state, launch=fake_launch)

    # idle, enough data, fresh pairs -> starts
    cfg.threshold_minutes = 0.01
    assert await autotrain.maybe_start(state, launch=fake_launch)
    assert len(launches) == 1


async def test_autotrain_skips_stale_and_active(client):
    state = client.app.state.una
    cfg = state.config.training
    launches = []

    async def fake_launch(st):
        launches.append(st)
        return {"id": "fake-run"}

    _make_eligible_pair(client)
    await _force_out_of_holdout(state)
    cfg.auto = True
    cfg.threshold_minutes = 0.01
    state.last_dictation_at = time.monotonic() - cfg.auto_idle_minutes * 60 - 1

    # A run attempt newer than every correction -> data is stale, no retrain.
    await state.db.execute(
        """INSERT INTO training_runs (id, status, started_at)
           VALUES ('r-old', 'rejected', '9999-01-01T00:00:00+00:00')"""
    )
    await state.db.commit()
    assert not await autotrain.maybe_start(state, launch=fake_launch)

    # An active run -> never double-start.
    await state.db.execute("UPDATE training_runs SET status = 'training', started_at = ''")
    await state.db.commit()
    assert not await autotrain.maybe_start(state, launch=fake_launch)
    assert launches == []

    # Run finished long ago -> the same pairs are fresh again relative to it.
    await state.db.execute("UPDATE training_runs SET status = 'rejected'")
    await state.db.commit()
    assert await autotrain.maybe_start(state, launch=fake_launch)
    assert len(launches) == 1
