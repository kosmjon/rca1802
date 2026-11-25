"""Stamp a cache-busting build version into web/index.html.

Rewrites the `?v=...` on the `js/main.js` script tag to a fresh value. That
version flows to every module and asset the page loads (each reads its own
`?v=` from `import.meta.url`), so one stamp busts the whole graph on deploy.

Run from the `emulator` folder (build-web.cmd and the Pages workflow do):
    python web/stamp-version.py [VERSION]
VERSION defaults to the current UTC time as YYYYMMDDHHMMSS.
"""
import datetime
import pathlib
import re
import sys

HERE = pathlib.Path(__file__).parent
INDEX = HERE / "index.html"

PATTERN = re.compile(r'(src="js/main\.js)(\?v=[^"]*)?(")')


def main():
    version = sys.argv[1] if len(sys.argv) > 1 else (
        datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%d%H%M%S")
    )
    text = INDEX.read_text(encoding="utf-8")
    new, n = PATTERN.subn(rf'\g<1>?v={version}\g<3>', text)
    if n != 1:
        raise SystemExit(
            f"stamp-version: expected exactly one js/main.js script tag, found {n}"
        )
    INDEX.write_text(new, encoding="utf-8")
    print(f"stamped build version {version} into {INDEX}")


if __name__ == "__main__":
    main()
