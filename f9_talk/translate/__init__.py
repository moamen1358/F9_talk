"""Translation backends. Lingva primary, MyMemory fallback."""
from f9_talk.translate.lingva import LingvaTranslator
from f9_talk.translate.mymemory import MyMemoryTranslator

__all__ = ["LingvaTranslator", "MyMemoryTranslator"]
