"""f9-talk — hold-to-talk dictation for Linux.

Side-effect on import: preload CUDA 12 cublas/cudnn shared objects when present.
ctranslate2 (used by faster-whisper for the local backend) links against
libcublas.so.12 / libcudnn.so.9 specifically. PyTorch wheels often pull cu13
libs alongside, so we explicitly load the cu12 variants here. Harmless when
the local backend isn't installed.
"""
from __future__ import annotations

import ctypes
import sys
from pathlib import Path

__version__ = "0.2.1"

_site = (
    Path(sys.prefix)
    / "lib"
    / f"python{sys.version_info.major}.{sys.version_info.minor}"
    / "site-packages"
    / "nvidia"
)

_PRELOAD = (
    "cublas/lib/libcublas.so.12",
    "cublas/lib/libcublasLt.so.12",
    "cudnn/lib/libcudnn.so.9",
    "cudnn/lib/libcudnn_ops.so.9",
    "cudnn/lib/libcudnn_cnn.so.9",
    "cuda_nvrtc/lib/libnvrtc.so.12",
)

for _rel in _PRELOAD:
    _p = _site / _rel
    if _p.exists():
        try:
            ctypes.CDLL(str(_p), mode=ctypes.RTLD_GLOBAL)
        except OSError:
            pass
