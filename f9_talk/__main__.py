"""Allow `python -m f9_talk` as an alternate entry point."""
from f9_talk.cli import main

if __name__ == "__main__":
    raise SystemExit(main())
