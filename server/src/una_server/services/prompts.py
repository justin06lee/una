"""Cleanup prompt construction + app-name -> tone mapping."""

from __future__ import annotations

from ..config import CleanupConfig

SYSTEM_TEMPLATE = """You clean up dictated speech transcripts. Output ONLY the cleaned text, nothing else.

Rules:
- Remove filler words (um, uh, like, you know) and false starts.
- Add punctuation and capitalization.
- NEVER paraphrase, summarize, or add content. Keep the speaker's words and their order otherwise.
- If the speaker self-corrects ("send it Monday, no wait, Tuesday"), keep only the corrected version.
{dictionary_clause}Tone: {tone}.

Examples:
Input: um so basically i think we should uh push the the release to friday
Output: I think we should push the release to Friday.
Input: hey can you send me the uh the figma link when you get a chance thanks
Output: Hey, can you send me the Figma link when you get a chance? Thanks!"""


def tone_for_app(cfg: CleanupConfig, app_name: str | None) -> str:
    if app_name:
        lowered = app_name.lower()
        for key, tone in cfg.tone_map.items():
            if key.lower() in lowered:
                return tone
    return cfg.default_tone


def build_system_prompt(cfg: CleanupConfig, phrases: list[str], app_name: str | None) -> str:
    dictionary_clause = ""
    if phrases:
        dictionary_clause = "- Spellings to use exactly when these terms occur: " + ", ".join(phrases) + ".\n"
    return SYSTEM_TEMPLATE.format(
        dictionary_clause=dictionary_clause,
        tone=tone_for_app(cfg, app_name),
    )


def build_initial_prompt(phrases: list[str], max_chars: int) -> str | None:
    """Whisper initial_prompt vocabulary biasing, hard-capped to avoid prompt-term hallucination."""
    if not phrases:
        return None
    out: list[str] = []
    length = len("Vocabulary: ")
    for phrase in phrases:
        extra = len(phrase) + 2
        if length + extra > max_chars:
            break
        out.append(phrase)
        length += extra
    if not out:
        return None
    return "Vocabulary: " + ", ".join(out) + "."
