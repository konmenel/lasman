#!/bin/env -S python3 -m
from ._clip import get_parser
from ._clip import main


__all__ = ["main", "get_parser"]

if __name__ == "__main__":
    raise SystemExit(main())
