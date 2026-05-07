#!/bin/bash
# f9-talk launcher.
# Sources API keys from ~/.config/F9_talk/secrets.env (or ./.env), then runs.
#
# Usage:
#   ./run.sh                            # default: cloud backend, F9 hold-to-talk
#   ./run.sh --backend local            # local Whisper on GPU
#   ./run.sh --backend both             # F9=local, F8=cloud
#   ./run.sh --target ar                # translate English speech → Arabic
#   ./run.sh --keyword Anthropic        # boost a domain term (repeatable)
#   ./run.sh --style ripple             # alternate indicator style
#   ./run.sh -v                         # debug logs
set -e

cd "$(dirname "$0")"

if [[ ! -f .venv/bin/python ]]; then
    echo "ERROR: .venv not found. Set it up with:" >&2
    echo "  python3 -m venv .venv && .venv/bin/pip install -e ." >&2
    echo "  # or, for the local backend too:" >&2
    echo "  .venv/bin/pip install -e '.[local]'" >&2
    exit 1
fi

exec .venv/bin/python -m f9_talk "$@"
