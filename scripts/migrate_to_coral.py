#!/usr/bin/env python3
"""One-shot migration script: reads Urchin MongoDB, POSTs to Coral API."""

import sys
import requests
from collections import defaultdict
from pymongo import MongoClient
from datetime import datetime, timezone

CORAL_API = "https://api.urchin.gg/v3"
BATCH_SIZE = 200

VALID_TAGS = {"sniper", "blatant_cheater", "closet_cheater", "confirmed_cheater"}

def parse_args():
    flags = set()
    api_key = None
    for a in sys.argv[1:]:
        if a.startswith("--"):
            flags.add(a)
        elif api_key is None:
            api_key = a
    return api_key, flags


def post(api_key, payload):
    r = requests.post(
        f"{CORAL_API}/migrate",
        json=payload,
        headers={"X-API-Key": api_key},
        timeout=120,
    )
    r.raise_for_status()
    return r.json()


def strip_uuid(val):
    if not val or not isinstance(val, str):
        return None
    return val.replace("-", "").lower()


def map_access_level(doc):
    if doc.get("is_admin"):
        return 4
    if doc.get("is_mod"):
        return 3
    if doc.get("private"):
        return 1
    return 0


def format_dt(val):
    if val is None:
        return None
    if isinstance(val, datetime):
        if val.tzinfo is None:
            val = val.replace(tzinfo=timezone.utc)
        return val.isoformat()
    if isinstance(val, str):
        if val.endswith("Z"):
            return val
        if "+" not in val and not val.endswith("Z"):
            return val + "+00:00"
        return val
    return None


def build_member(doc):
    mc_accounts = []
    for uuid in doc.get("minecraft_accounts", []) or []:
        cleaned = strip_uuid(uuid)
        if cleaned and len(cleaned) == 32:
            mc_accounts.append(cleaned)

    return {
        "discord_id": int(doc["discord_id"]),
        "uuid": strip_uuid(doc.get("uuid")),
        "join_date": format_dt(doc.get("join_date")),
        "request_count": doc.get("request_count", 0) or 0,
        "access_level": map_access_level(doc),
        "key_locked": doc.get("key_locked", False) or False,
        "config": doc.get("config") or {},
        "minecraft_accounts": mc_accounts,
    }


def map_tag(tag):
    tag_type = tag.get("tag_type", "")
    reason = tag.get("reason", "")
    added_by = tag.get("added_by")

    try:
        added_by = int(added_by)
    except (ValueError, TypeError):
        added_by = None

    if not added_by:
        return None

    if tag_type in ("caution", "account"):
        if "replays needed" in reason.lower():
            return {
                "tag_type": "replays_needed",
                "reason": "",
                "added_by": added_by,
                "added_on": format_dt(tag.get("added_on")),
                "hide_username": tag.get("hide_username", False) or False,
            }
        return None

    if tag_type not in VALID_TAGS:
        return None

    return {
        "tag_type": tag_type,
        "reason": reason,
        "added_by": added_by,
        "added_on": format_dt(tag.get("added_on")),
        "hide_username": tag.get("hide_username", False) or False,
    }


def collect_blacklist(db):
    """Merge duplicate UUID documents and deduplicate tags by type."""
    players = defaultdict(lambda: {"tags": [], "lock": None})

    for doc in db.blacklist.find():
        uuid = strip_uuid(doc.get("uuid"))
        if not uuid or len(uuid) != 32:
            continue

        entry = players[uuid]

        for tag in doc.get("tags", []) or []:
            mapped = map_tag(tag)
            if mapped:
                entry["tags"].append(mapped)

        if doc.get("is_locked") and not entry["lock"]:
            locked_at = None
            ts = doc.get("lock_timestamp")
            if isinstance(ts, datetime):
                if ts.tzinfo is None:
                    ts = ts.replace(tzinfo=timezone.utc)
                locked_at = ts.isoformat()

            locked_by = None
            raw = doc.get("locked_by")
            if raw:
                try:
                    locked_by = int(raw)
                except (ValueError, TypeError):
                    pass

            entry["lock"] = {
                "is_locked": True,
                "lock_reason": doc.get("lock_reason"),
                "locked_by": locked_by,
                "locked_at": locked_at,
                "evidence_thread": doc.get("evidence_thread"),
            }

    results = []
    for uuid, entry in players.items():
        seen_types = set()
        deduped = []
        for tag in entry["tags"]:
            if tag["tag_type"] not in seen_types:
                seen_types.add(tag["tag_type"])
                deduped.append(tag)

        if not deduped:
            continue

        lock = entry["lock"] or {}
        results.append({
            "uuid": uuid,
            "is_locked": lock.get("is_locked", False),
            "lock_reason": lock.get("lock_reason"),
            "locked_by": lock.get("locked_by"),
            "locked_at": lock.get("locked_at"),
            "evidence_thread": lock.get("evidence_thread"),
            "tags": deduped,
        })

    return results


def migrate_members(db, api_key):
    print("Migrating members...")
    batch = []
    total = 0
    errors = 0

    for doc in db.members.find():
        batch.append(build_member(doc))

        if len(batch) >= BATCH_SIZE:
            result = post(api_key, {"type": "members", "data": batch})
            total += result["migrated"]
            errors += result["errors"]
            print(f"  {total} members migrated ({errors} errors)")
            batch = []

    if batch:
        result = post(api_key, {"type": "members", "data": batch})
        total += result["migrated"]
        errors += result["errors"]

    print(f"Members done: {total} migrated, {errors} errors")


def migrate_blacklist(db, api_key):
    print("Collecting and merging blacklist...")
    players = collect_blacklist(db)
    print(f"  {len(players)} unique players with valid tags")

    total = 0
    errors = 0

    for i in range(0, len(players), BATCH_SIZE):
        batch = players[i:i + BATCH_SIZE]
        result = post(api_key, {"type": "blacklist", "data": batch})
        total += result["migrated"]
        errors += result["errors"]
        print(f"  {total} players migrated ({errors} errors)")

    print(f"Blacklist done: {total} migrated, {errors} errors")


def main():
    api_key, flags = parse_args()
    if not api_key or not flags - {"--wipe"}:
        print("Usage: python3 migrate_to_coral.py <INTERNAL_API_KEY> [--wipe] [--members] [--blacklist]")
        print("  --wipe        Wipe all migrated data first")
        print("  --members     Migrate members")
        print("  --blacklist   Migrate blacklist")
        sys.exit(1)

    db = MongoClient("mongodb://localhost:27017").urchindb

    if "--members" in flags:
        if "--wipe" in flags:
            print("Wiping members...")
            print(f"  {post(api_key, {'type': 'wipe_members'})}")
        migrate_members(db, api_key)

    if "--blacklist" in flags:
        if "--wipe" in flags:
            print("Wiping blacklist...")
            print(f"  {post(api_key, {'type': 'wipe_blacklist'})}")
        migrate_blacklist(db, api_key)

    print("Done!")


if __name__ == "__main__":
    main()
