#!/usr/bin/env python3
"""Refuse to deploy a static site with a missing entry page or a broken local link.

Every href/src that is not an external URL, a mail/tel link or a pure #fragment must
resolve to a file inside the site directory; links that escape the directory fail too.
"""

import argparse
from html.parser import HTMLParser
import json
from pathlib import Path
import sys
from urllib.parse import unquote, urlsplit

EXTERNAL = ("http", "https", "mailto", "tel", "data", "javascript")


class Links(HTMLParser):
    def __init__(self):
        super().__init__()
        self.targets = []

    def handle_starttag(self, tag, attrs):
        for name, value in attrs:
            if name in ("href", "src") and value:
                self.targets.append(value.strip())


def broken_links(site):
    site = Path(site).resolve()
    problems = []
    if not (site / "index.html").is_file():
        problems.append("index.html is missing")
    pages = sorted(site.rglob("*.html"))
    if not pages:
        problems.append("no HTML pages found")
    for page in pages:
        parser = Links()
        parser.feed(page.read_text(encoding="utf-8"))
        for target in parser.targets:
            parts = urlsplit(target)
            if parts.scheme in EXTERNAL or target.startswith("//") or not parts.path:
                continue
            path = unquote(parts.path)
            resolved = (site / path.lstrip("/")) if path.startswith("/") else (page.parent / path)
            resolved = resolved.resolve()
            if resolved.is_dir():
                resolved = resolved / "index.html"
            where = page.relative_to(site).as_posix()
            if not resolved.is_relative_to(site):
                problems.append(f"{where}: {target} points outside the site")
            elif not resolved.is_file():
                problems.append(f"{where}: {target} does not exist")
    return len(pages), problems


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("site", type=Path)
    args = parser.parse_args(argv)
    if not args.site.is_dir():
        print(f"::error::Site directory {args.site} does not exist", file=sys.stderr)
        return 1
    pages, problems = broken_links(args.site)
    for problem in problems:
        print(f"::error title=Site check::{problem}", file=sys.stderr)
    print(json.dumps({"event": "site_check", "pages": pages, "problems": len(problems)}))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
