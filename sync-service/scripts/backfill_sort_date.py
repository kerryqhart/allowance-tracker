#!/usr/bin/env python3
"""Backfill the `sort_date` GSI key on the transactions table to the canonical
fixed-width UTC-nanosecond format produced by `normalize_sort_date` in
sync-service/src/storage/dynamo.rs.

Read-only by default. Pass --apply to write UpdateItem changes.

    python3 backfill_sort_date.py                 # dry run
    python3 backfill_sort_date.py --apply          # write changes

Region/table can be overridden with --region / --table.
"""
import argparse
import json
import re
import subprocess
import sys

DATE_RE = re.compile(
    r"^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})$"
)


def normalize_sort_date(s: str) -> str:
    """Mirror of the Rust normalize_sort_date: parse RFC3339, convert to UTC,
    emit fixed 9-digit nanoseconds + 'Z'. Falls back to the raw string."""
    import datetime

    m = DATE_RE.match(s)
    if not m:
        return s
    base, frac, tz = m.groups()
    nanos = ((frac[1:] if frac else "") + "000000000")[:9]
    dt = datetime.datetime.strptime(base, "%Y-%m-%dT%H:%M:%S")
    if tz == "Z":
        offset = datetime.timedelta(0)
    else:
        sign = 1 if tz[0] == "+" else -1
        offset = sign * datetime.timedelta(hours=int(tz[1:3]), minutes=int(tz[4:6]))
    utc = dt - offset  # whole-minute offsets only, so sub-second nanos are preserved
    return utc.strftime("%Y-%m-%dT%H:%M:%S") + "." + nanos + "Z"


def aws(args):
    return subprocess.run(
        ["aws"] + args, capture_output=True, text=True, check=True
    ).stdout


def scan_all(region, table):
    items, start_key = [], None
    while True:
        cmd = ["dynamodb", "scan", "--region", region, "--table-name", table,
               "--output", "json"]
        if start_key:
            cmd += ["--exclusive-start-key", json.dumps(start_key)]
        out = json.loads(aws(cmd))
        items.extend(out.get("Items", []))
        start_key = out.get("LastEvaluatedKey")
        if not start_key:
            return items


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--region", default="us-east-2")
    ap.add_argument("--table", default="allowance-tracker-transactions")
    ap.add_argument("--apply", action="store_true")
    a = ap.parse_args()

    items = scan_all(a.region, a.table)
    changed = 0
    for it in items:
        cid = it["child_id"]["S"]
        tid = it["transaction_id"]["S"]
        cur = it.get("sort_date", {}).get("S")
        data = it.get("data", {}).get("S")
        if not data:
            continue
        date_str = json.loads(data).get("date")
        if not date_str:
            continue
        want = normalize_sort_date(date_str)
        if cur == want:
            continue
        changed += 1
        print(f"{tid:<55} {str(cur):<38} -> {want}")
        if a.apply:
            aws(["dynamodb", "update-item", "--region", a.region,
                 "--table-name", a.table,
                 "--key", json.dumps({"child_id": {"S": cid},
                                      "transaction_id": {"S": tid}}),
                 "--update-expression", "SET sort_date = :v",
                 "--expression-attribute-values",
                 json.dumps({":v": {"S": want}})])

    mode = "APPLIED" if a.apply else "DRY RUN (no writes)"
    print(f"\n{mode}: {changed} of {len(items)} rows need sort_date normalization")


if __name__ == "__main__":
    sys.exit(main())
