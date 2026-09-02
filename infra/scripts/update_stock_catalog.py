#!/usr/bin/env python3
"""Download KIS domestic stock master files and build data/stocks.json."""

from __future__ import annotations

import argparse
import json
import ssl
import tempfile
import urllib.error
import urllib.request
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
OUTPUT_PATH = ROOT / "data" / "stocks.json"
MASTERS = [
    (
        "KOSPI",
        "https://new.real.download.dws.co.kr/common/master/kospi_code.mst.zip",
        228,
    ),
    (
        "KOSDAQ",
        "https://new.real.download.dws.co.kr/common/master/kosdaq_code.mst.zip",
        222,
    ),
]


def download(url: str) -> bytes:
    try:
        with urllib.request.urlopen(url, timeout=20) as response:
            return response.read()
    except urllib.error.URLError as error:
        if not isinstance(error.reason, ssl.SSLCertVerificationError):
            raise

        context = ssl._create_unverified_context()
        with urllib.request.urlopen(url, timeout=20, context=context) as response:
            return response.read()


def parse_master(content: bytes, market: str, tail_size: int) -> list[dict[str, str]]:
    rows: list[dict[str, str]] = []

    with tempfile.TemporaryDirectory() as temp_dir:
        zip_path = Path(temp_dir) / "master.zip"
        zip_path.write_bytes(content)

        with zipfile.ZipFile(zip_path) as archive:
            names = archive.namelist()
            if not names:
                return rows
            raw = archive.read(names[0]).decode("cp949")

    for line in raw.splitlines():
        base = line[: len(line) - tail_size]
        symbol = base[:9].strip()
        name = base[21:].strip()

        if len(symbol) == 6 and symbol.isdigit() and name:
            rows.append({"symbol": symbol, "name": name, "market": market})

    return rows


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Build data/stocks.json from KIS stock masters.")
    parser.add_argument(
        "--output",
        default=str(OUTPUT_PATH),
        help="Path to write the stock catalog JSON.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    output_path = Path(args.output)
    stocks: dict[str, dict[str, str]] = {}

    for market, url, tail_size in MASTERS:
        for stock in parse_master(download(url), market, tail_size):
            stocks[stock["symbol"]] = stock

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(
        json.dumps(
            sorted(stocks.values(), key=lambda item: item["symbol"]),
            ensure_ascii=False,
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"wrote {len(stocks)} stocks to {output_path}")


if __name__ == "__main__":
    main()
