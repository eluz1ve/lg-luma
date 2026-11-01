#!/usr/bin/env python3
"""Writes assets/apple-tv.json: the Apple TV screensaver places that have a full-size still.

Reads Apple's tvOS aerials catalog (entries.json and the place names in Luma's languages, from
the resources tar Apple TV downloads) and keeps the places whose 1920 x 1080 snapshot Apple
serves. Luma downloads the snapshots themselves on the TV, from Apple, as it shows them.

Run: python3 tools/apple-tv-wallpapers.py
"""
import concurrent.futures
import io
import json
import pathlib
import plistlib
import tarfile
import urllib.request

FEED = (
    "https://sylvan.apple.com/itunes-assets/Aerials126/v4/c0/45/d9/"
    "c045d9d0-9606-1535-62fe-189edb4f79eb/resources-atv-23J-2.tar"
)
SNAPSHOTS = "https://sylvan.apple.com/Aerials/jjGqDwCNrOsVJ4of6uSVr/snapshots/"
# Luma's languages and the folders Apple keeps their names in.
LANGUAGES = {
    "en": "en", "de": "de", "zh": "zh_CN", "zh-tw": "zh_TW", "es": "es", "pt": "pt_BR",
    "it": "it", "ru": "ru", "pl": "pl", "fr": "fr", "vi": "vi", "nl": "nl", "sv": "sv",
}
OUT = pathlib.Path(__file__).resolve().parent.parent / "assets" / "apple-tv.json"


def fetch(url, method="GET"):
    request = urllib.request.Request(url, method=method, headers={"User-Agent": "LumaNative"})
    with urllib.request.urlopen(request, timeout=30) as response:
        return response.status, response.read()


def has_snapshot(shot):
    try:
        return fetch(f"{SNAPSHOTS}{shot}.jpg", "HEAD")[0] == 200
    except OSError:
        return False


def main():
    archive = tarfile.open(fileobj=io.BytesIO(fetch(FEED)[1]))

    def member(name):
        return archive.extractfile(f"./{name}").read()

    entries = json.loads(member("entries.json"))
    strings = {
        code: plistlib.loads(member(f"TVIdleScreenStrings.bundle/{folder}.lproj/Localizable.nocache.strings"))
        for code, folder in LANGUAGES.items()
    }
    assets = entries["assets"]
    with concurrent.futures.ThreadPoolExecutor(8) as pool:
        served = list(pool.map(has_snapshot, (asset["shotID"] for asset in assets)))
    pictures, seen = [], set()
    for asset, ok in zip(assets, served):
        shot = asset["shotID"]
        if not ok or shot in seen:
            continue
        seen.add(shot)
        key = asset["localizedNameKey"]
        fallback = asset.get("accessibilityLabel") or strings["en"].get(key, shot)
        pictures.append({"id": shot, "name": {code: names.get(key, fallback) for code, names in strings.items()}})
    lines = ",\n".join("    " + json.dumps(picture, ensure_ascii=False) for picture in pictures)
    OUT.write_text(
        '{\n  "feed": ' + json.dumps(FEED) + ',\n  "snapshots": ' + json.dumps(SNAPSHOTS)
        + ',\n  "pictures": [\n' + lines + "\n  ]\n}\n"
    )
    print(f"{len(pictures)} of {len(assets)} places have a full-size still -> {OUT}")


if __name__ == "__main__":
    main()
