"""Read-only photo archiver predicates; stdout is captured machine data."""

import argparse
import subprocess
import sys
from dataclasses import dataclass

from common import (
    Batch,
    IntegrityError,
    ObservationError,
    Settings,
    batch_matches,
    compact,
    discover_card,
    photo_files,
    scan_photos,
    verify_archive,
)
from session import Session, read_manifest, read_receipt, read_session

SENSOR_NAMES = (
    "sd_card_present",
    "photo_batch",
    "photos_copied",
    "archive_verified",
    "user_notified",
    "import_failed",
    "session_obsolete",
    "idle",
)


@dataclass(frozen=True)
class ProbeResult:
    """Exit 0 true, 1 false; CLI observation exceptions become exit 2."""

    exit_code: int
    stdout: str = ""


def predicate(value: bool, stdout: str = "") -> ProbeResult:
    """Return data only when its predicate is true."""
    return ProbeResult(0, stdout) if value else ProbeResult(1)


def _sense_idle(
    batch: Batch | None, session: Session | None, matching: bool
) -> ProbeResult:
    """Keep obsolete sessions pending before considering an empty card."""
    if session is not None and not matching:
        return predicate(False)
    if batch is None or not batch.files:
        return predicate(True)
    return predicate(
        session is not None
        and matching
        and (session.status == "failed" or read_receipt(session) is not None)
    )


def _sense_photos_copied(batch: Batch, session: Session) -> ProbeResult:
    """Compare manifest, destination, and source photo inventories."""
    manifest = read_manifest(session.destination)
    if manifest != session.without_receipt():
        raise ObservationError("copy manifest/session mismatch")
    expected = {entry.relative_path: entry.size for entry in session.files}
    observed = {
        photo.relative_path: photo.size
        for photo in photo_files(session.destination)
    }
    inventory = {photo.relative_path: photo.size for photo in batch.files}
    return predicate(expected == observed == inventory)


def _sense_archive_verified(batch: Batch, session: Session) -> ProbeResult:
    """Reuse a receipt or verify the current archive without writing state."""
    receipt = read_receipt(session)
    if receipt is not None:
        return predicate(True, compact(receipt.record()))
    try:
        proof = verify_archive(batch, session)
    except (IntegrityError, FileNotFoundError) as error:
        print(f"archive unverified: {error}", file=sys.stderr)
        return predicate(False)
    return predicate(True, compact(proof.record()))


def sense(name: str, settings: Settings) -> ProbeResult:
    """Observe independently without consuming other sensors' captures."""
    card = discover_card(settings)
    if name == "sd_card_present":
        return predicate(card is not None)
    batch = scan_photos(card) if card is not None else None
    if name == "photo_batch":
        return predicate(
            batch is not None and bool(batch.files),
            compact(batch.capture()) if batch is not None else "",
        )
    session = read_session(settings)
    matching = (
        session is not None
        and batch is not None
        and batch_matches(batch, session)
    )
    match name:
        case "session_obsolete":
            return predicate(
                session is not None and not matching,
                session.session_id if session is not None else "",
            )
        case "import_failed":
            return predicate(
                matching and session is not None and session.status == "failed"
            )
        case "idle":
            return _sense_idle(batch, session, matching)
    if (
        session is None
        or batch is None
        or not matching
        or session.status != "copied"
    ):
        return predicate(False)
    match name:
        case "photos_copied":
            return _sense_photos_copied(batch, session)
        case "user_notified":
            return predicate(read_receipt(session) is not None)
        case "archive_verified":
            return _sense_archive_verified(batch, session)
        case _:
            raise ObservationError(f"unknown sensor: {name}")


def main() -> int:
    """Print successful machine captures, diagnostics only to stderr."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("sensor", choices=SENSOR_NAMES)
    args = parser.parse_args()
    try:
        result = sense(args.sensor, Settings.from_env())
        if result.stdout:
            print(result.stdout)
        return result.exit_code
    except (ObservationError, OSError, subprocess.SubprocessError) as error:
        print(f"observation error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
