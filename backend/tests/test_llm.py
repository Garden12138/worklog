from datetime import date

from app.services.llm import build_chat_payload


def test_build_chat_payload_is_openai_compatible():
    payload = build_chat_payload(
        "gpt-test",
        "周报",
        (date(2026, 6, 22), date(2026, 6, 28)),
        "- 记录",
    )
    assert payload["model"] == "gpt-test"
    assert payload["messages"][0]["role"] == "system"
    assert payload["messages"][1]["role"] == "user"
    assert "2026-06-22" in payload["messages"][1]["content"]
