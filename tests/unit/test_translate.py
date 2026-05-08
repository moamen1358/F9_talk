"""Unit tests for translation backends (HTTP mocked)."""
from unittest.mock import MagicMock, patch

import pytest

from f9_talk.translate.lingva import LingvaTranslator
from f9_talk.translate.mymemory import MyMemoryTranslator


# ---------- LingvaTranslator ----------

def test_lingva_rejects_invalid_src_lang():
    with pytest.raises(ValueError, match="Invalid source"):
        LingvaTranslator(src_lang="en/../../evil", target_lang="ar")


def test_lingva_rejects_invalid_tgt_lang():
    with pytest.raises(ValueError, match="Invalid target"):
        LingvaTranslator(src_lang="en", target_lang="ar/../bad")


def test_lingva_accepts_valid_lang_codes():
    t = LingvaTranslator(src_lang="en", target_lang="ar")
    assert t.src_lang == "en"
    assert t.target_lang == "ar"


def test_lingva_returns_same_text_for_same_language():
    t = LingvaTranslator(src_lang="en", target_lang="en")
    assert t.translate("hello") == "hello"


def test_lingva_empty_input_returns_empty():
    t = LingvaTranslator(src_lang="en", target_lang="ar")
    assert t.translate("") == ""
    assert t.translate("   ") == ""


def test_lingva_successful_translation():
    t = LingvaTranslator(src_lang="en", target_lang="ar")
    mock_response = MagicMock()
    mock_response.json.return_value = {"translation": "مرحبا"}
    mock_response.raise_for_status = MagicMock()

    with patch.object(t._session, "get", return_value=mock_response):
        result = t.translate("hello")

    assert result == "مرحبا"


def test_lingva_falls_back_to_mymemory_on_failure():
    t = LingvaTranslator(src_lang="en", target_lang="ar")

    with patch.object(t, "_translate_lingva", side_effect=RuntimeError("network error")), \
         patch.object(t, "_translate_fallback", return_value="fallback result") as mock_fb:
        result = t.translate("hello")

    mock_fb.assert_called_once_with("hello")
    assert result == "fallback result"


def test_lingva_returns_original_text_when_both_fail():
    t = LingvaTranslator(src_lang="en", target_lang="ar")

    with patch.object(t, "_translate_lingva", side_effect=RuntimeError("lingva down")), \
         patch.object(t, "_translate_fallback", side_effect=RuntimeError("mymemory down")):
        result = t.translate("hello")

    assert result == "hello"


# ---------- MyMemoryTranslator ----------

def test_mymemory_returns_same_text_for_same_language():
    t = MyMemoryTranslator(src_lang="en", target_lang="en")
    assert t.translate("hello") == "hello"


def test_mymemory_empty_input_returns_empty():
    t = MyMemoryTranslator(src_lang="en", target_lang="ar")
    assert t.translate("") == ""


def test_mymemory_successful_translation():
    t = MyMemoryTranslator(src_lang="en", target_lang="ar")
    mock_response = MagicMock()
    mock_response.json.return_value = {
        "responseStatus": "200",
        "responseData": {"translatedText": "مرحبا"},
    }
    mock_response.raise_for_status = MagicMock()

    with patch.object(t._session, "get", return_value=mock_response):
        result = t.translate("hello")

    assert result == "مرحبا"


def test_mymemory_returns_original_on_api_error():
    t = MyMemoryTranslator(src_lang="en", target_lang="ar")
    mock_response = MagicMock()
    mock_response.json.return_value = {
        "responseStatus": "429",
        "responseDetails": "quota exceeded",
        "responseData": {"translatedText": ""},
    }
    mock_response.raise_for_status = MagicMock()

    with patch.object(t._session, "get", return_value=mock_response):
        result = t.translate("hello")

    assert result == "hello"


def test_mymemory_includes_email_in_params_when_set():
    t = MyMemoryTranslator(src_lang="en", target_lang="ar", email="test@example.com")
    mock_response = MagicMock()
    mock_response.json.return_value = {
        "responseStatus": "200",
        "responseData": {"translatedText": "مرحبا"},
    }
    mock_response.raise_for_status = MagicMock()

    with patch.object(t._session, "get", return_value=mock_response) as mock_get:
        t.translate("hello")

    call_kwargs = mock_get.call_args[1]
    assert call_kwargs["params"]["de"] == "test@example.com"
