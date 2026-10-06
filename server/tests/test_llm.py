"""The Messages API client used to back-translate imported writing."""

import pytest

from una_server.services.llm import LlmError, extract_json


def test_extract_json_finds_the_object_among_prose_and_fences():
    reply = 'Sure!\n```json\n{"literal": "a", "polished": "b"}\n```'
    assert extract_json(reply) == {"literal": "a", "polished": "b"}
    with pytest.raises(LlmError):
        extract_json("no json here")
