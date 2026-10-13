"""Identity-bound, source-preserving photo archiver mutations."""

import argparse
import hashlib
import json
import os
import stat
import subprocess
import sys
import uuid
from dataclasses import replace
from datetime import datetime
from pathlib import Path

from common import (
    CHUNK_SIZE, MANIFEST_NAME, Batch, ObservationError, Photo, Settings,
    batch_matches, checked_path, compact, discover_card, file_digest,
    open_regular, safe_relative, scan_photos, verify_archive,
)
from session import (
    ArchiveEntry, Session, action_lock, ensure_directory, object_record,
    read_manifest, read_session, sync_directory, write_json_atomic,
    write_session_atomic,
)


class ActionError(ValueError):
    """A mutating operation is not authorized or cannot complete safely."""


def current_batch(settings: Settings, captured: dict[str, object]) -> Batch:
    """Reobserve the full compact captured identity before mutation."""
    card = discover_card(settings)
    if card is None:
        raise ActionError("captured card is no longer mounted")
    batch = scan_photos(card)
    if not batch.files or compact(captured) != compact(batch.capture()):
        raise ActionError("captured batch differs from current source")
    return batch


def allocate_session(settings: Settings, batch: Batch) -> Session:
    """Choose one dated destination; do not reuse an existing directory."""
    timestamp = datetime.now().strftime("%Y-%m-%d_%H-%M-%S")
    session_id = uuid.uuid4().hex
    destination = checked_path(settings.pictures_dir / f"{timestamp}-{session_id}")
    if destination.exists():
        raise ActionError("new session destination already exists")
    session = Session(
        session_id, batch.card, batch.inventory_id, timestamp,
        destination, "copying", (),
    )
    write_session_atomic(settings, session)
    ensure_directory(settings.pictures_dir)
    destination.mkdir()
    sync_directory(settings.pictures_dir)
    return session


def mark_failed(settings: Settings, session: Session, error: BaseException) -> None:
    """Fail closed only for an active session owned by this action."""
    active = read_session(settings)
    if active is not None and active.session_id == session.session_id:
        failed = replace(active, status="failed", failure_detail=str(error), notification_receipt=None)
        write_session_atomic(settings, failed)


def copy_photo(settings: Settings, session: Session, photo: Photo) -> Session:
    """Stream an owned temporary; journal its hash before publication."""
    relative = safe_relative(photo.relative_path)
    source = session.card.mount_path / relative
    destination = checked_path(session.destination / relative)
    recorded = {entry.relative_path: entry for entry in session.files}
    prior = recorded.get(photo.relative_path)
    if destination.exists():
        if prior is None or file_digest(destination) != (prior.size, prior.sha256):
            raise ActionError(f"refusing mismatching/unrecorded completed file: {photo.relative_path!r}")
        if file_digest(source) != (prior.size, prior.sha256):
            raise ActionError(f"source bytes changed: {photo.relative_path!r}")
        return session
    ensure_directory(destination.parent)
    path_id = hashlib.sha256(photo.relative_path.encode()).hexdigest()
    temporary = checked_path(destination.parent / f".uncharles-{session.session_id}-{path_id}.part")
    digest = hashlib.sha256()
    size = 0
    descriptor = os.open(
        temporary, os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW | os.O_NONBLOCK,
        0o600,
    )
    try:
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise ActionError("owned temporary is not a regular file")
        os.ftruncate(descriptor, 0)
        output = os.fdopen(descriptor, "wb")
    except (OSError, ActionError):
        os.close(descriptor)
        raise
    with output, open_regular(source) as stream:
        before = os.fstat(stream.fileno())
        if (before.st_size, before.st_mtime_ns) != (photo.size, photo.mtime_ns):
            raise ActionError("source metadata changed before copying")
        while chunk := stream.read(CHUNK_SIZE):
            output.write(chunk)
            digest.update(chunk)
            size += len(chunk)
        after = os.fstat(stream.fileno())
        if (after.st_size, after.st_mtime_ns) != (photo.size, photo.mtime_ns) or size != photo.size:
            raise ActionError("source metadata changed during copying")
        output.flush()
        os.fsync(output.fileno())
    entry = ArchiveEntry(photo.relative_path, size, digest.hexdigest())
    if prior is not None and prior != entry:
        raise ActionError("source no longer matches previously journaled bytes")
    recorded[photo.relative_path] = entry
    session = replace(session, files=tuple(recorded[key] for key in sorted(recorded)))
    # Write-ahead progress reconciles a crash on either side of publication:
    # a missing final is retried; a present final must match this saved hash.
    write_session_atomic(settings, session)
    os.link(temporary, destination, follow_symlinks=False)
    sync_directory(destination.parent)
    temporary.unlink()
    sync_directory(destination.parent)
    return session


def complete_copy(settings: Settings, batch: Batch, session: Session) -> Session:
    """Finish immutable evidence only after all progress/source checks."""
    current_batch(settings, batch.capture())
    expected = {photo.relative_path for photo in batch.files}
    if {entry.relative_path for entry in session.files} != expected:
        raise ActionError("copy progress does not cover the complete inventory")
    completed = replace(session, status="copied", failure_detail=None)
    manifest_path = session.destination / MANIFEST_NAME
    if manifest_path.exists():
        if read_manifest(session.destination) != completed:
            raise ActionError("existing completed manifest differs from session")
    else:
        write_json_atomic(manifest_path, completed.record(), immutable=True)
    write_session_atomic(settings, completed)
    return completed


def copy_batch(settings: Settings, captured_batch: dict[str, object]) -> Session:
    """Copy or resume only a matching batch, under the action writer lock."""
    current_batch(settings, captured_batch)
    with action_lock(settings):
        batch = current_batch(settings, captured_batch)
        session = read_session(settings)
        if session is not None and not batch_matches(batch, session):
            raise ActionError("obsolete session must be reset before copying")
        if session is not None and session.status == "failed":
            raise ActionError("failed session requires explicit operator recovery")
        try:
            if session is None:
                session = allocate_session(settings, batch)
            if session.status == "copied":
                verify_archive(batch, session)
                return session
            ensure_directory(session.destination)
            for photo in batch.files:
                session = copy_photo(settings, session, photo)
            return complete_copy(settings, batch, session)
        except (ActionError, ObservationError, OSError) as error:
            # Read the latest write-ahead progress, including allocation
            # failure after the initial state record was committed.
            active = read_session(settings)
            if active is not None and batch_matches(batch, active):
                mark_failed(settings, active, error)
            raise


def captured_env(name: str) -> dict[str, object]:
    """Parse required captured JSON without shell-expanded paths."""
    value = os.environ.get(name)
    if value is None:
        raise ActionError(f"missing capture: {name}")
    try:
        return object_record(json.loads(value))
    except json.JSONDecodeError as error:
        raise ActionError(f"invalid captured JSON: {name}") from error


def main() -> int:
    """Run a guarded action; print failures to stderr and exit one."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("copy",))
    args = parser.parse_args()
    try:
        settings = Settings.from_env()
        if args.action == "copy":
            copy_batch(settings, captured_env("UNCHARLES_FACT_PHOTO_BATCH"))
        return 0
    except (ActionError, ObservationError, OSError, subprocess.SubprocessError) as error:
        print(f"action error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
