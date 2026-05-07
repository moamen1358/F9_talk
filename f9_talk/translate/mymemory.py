"""MyMemory free translation API. 5K chars/day anonymous, 50K with email."""
from __future__ import annotations

import logging
import os

import requests

log = logging.getLogger(__name__)


class MyMemoryTranslator:
    URL = "https://api.mymemory.translated.net/get"

    def __init__(
        self,
        src_lang: str = "en",
        target_lang: str = "ar",
        email: str | None = None,
        timeout: float = 5.0,
    ) -> None:
        self.src_lang = src_lang
        self.target_lang = target_lang
        self.email = email or os.environ.get("MYMEMORY_EMAIL")
        self.timeout = timeout
        self._session = requests.Session()

    def set_pair(self, src: str, tgt: str) -> None:
        self.src_lang = src
        self.target_lang = tgt

    def translate(self, text: str) -> str:
        text = (text or "").strip()
        if not text:
            return ""
        if self.src_lang == self.target_lang:
            return text
        params: dict = {"q": text, "langpair": f"{self.src_lang}|{self.target_lang}"}
        if self.email:
            params["de"] = self.email
        r = self._session.get(self.URL, params=params, timeout=self.timeout)
        r.raise_for_status()
        data = r.json()
        if str(data.get("responseStatus")) != "200":
            log.warning("MyMemory error: %s", data.get("responseDetails", "unknown"))
            return text
        return data["responseData"]["translatedText"]
