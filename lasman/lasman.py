#!/bin/env -S python3 -m
import argparse

from . import clip

TOOLS = [clip]


def get_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser("lasman")

    subparser = parser.add_subparsers(title="Tools", required=True)
    for tool in TOOLS:
        tool_parser = tool.get_parser()
        tool_parser.set_defaults(func=tool.main)
        subparser.add_parser(
            tool_parser.prog,
            add_help=False,
            parents=[tool_parser],
            help=tool_parser.description,
        )
    return parser


def main() -> int:
    parser = get_parser()
    args = parser.parse_args()
    args.func(args)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
