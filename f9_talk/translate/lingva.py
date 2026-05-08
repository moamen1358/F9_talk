"""Lingva.ml translator — public Google Translate proxy. Free, no API key.

Falls back to MyMemory automatically if Lingva is unreachable.
"""
from __future__ import annotations

import logging
import re
from urllib.parse import quote

import requests

_LANG_RE = re.compile(r"^[a-zA-Z]{2,5}$")

log = logging.getLogger(__name__)


class LingvaTranslator:
    URL_TEMPLATE = "https://lingva.ml/api/v1/{src}/{tgt}/{text}"

    def __init__(
        self,
        src_lang: str = "en",
        target_lang: str = "ar",
        timeout: float = 4.0,
    ) -> None:
        if not _LANG_RE.match(src_lang):
            raise ValueError(f"Invalid source language code: {src_lang!r}")
        if not _LANG_RE.match(target_lang):
            raise ValueError(f"Invalid target language code: {target_lang!r}")
        self.src_lang = src_lang
        self.target_lang = target_lang
        self.timeout = timeout
        self._session = requests.Session()
        self._fallback = None  # MyMemoryTranslator, lazily constructed
        log.info("Lingva translator ready (%s -> %s)", src_lang, target_lang)

    def set_pair(self, src: str, tgt: str) -> None:
        self.src_lang = src
        self.target_lang = tgt
        if self._fallback is not None:
            self._fallback.set_pair(src, tgt)

    def _translate_lingva(self, text: str) -> str:
        url = self.URL_TEMPLATE.format(
            src=self.src_lang,
            tgt=self.target_lang,
            text=quote(text, safe=""),
        )
        r = self._session.get(url, timeout=self.timeout)
        r.raise_for_status()
        data = r.json()
        out = (data.get("translation") or "").strip()
        if not out:
            raise RuntimeError("empty translation")
        return out

    def _translate_fallback(self, text: str) -> str:
        if self._fallback is None:
            from f9_talk.translate.mymemory import MyMemoryTranslator

            self._fallback = MyMemoryTranslator(
                src_lang=self.src_lang, target_lang=self.target_lang
            )
        return self._fallback.translate(text)

    def translate(self, text: str) -> str:
        text = (text or "").strip()
        if not text:
            return ""
        if self.src_lang == self.target_lang:
            return text
        try:
            return self._translate_lingva(text)
        except Exception as e:  # noqa: BLE001
            log.warning("lingva failed (%s); using MyMemory fallback", e)
            try:
                return self._translate_fallback(text)
            except Exception as e2:  # noqa: BLE001
                log.error("fallback failed too: %s", e2)
                return text
