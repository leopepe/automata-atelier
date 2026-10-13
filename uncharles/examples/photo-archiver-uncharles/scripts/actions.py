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

from common import (
    CHUNK_SIZE,
    MANIFEST_NAME,
    RECEIPT_NAME,
    Batch,
    ObservationError,
    Photo,
    Settings,
    batch_matches,
    checked_path,
    compact,
    discover_card,
    file_digest,
    open_regular,
    safe_relative,
    scan_photos,
    verify_archive,
)
from session import (
    ACTIVE_NAME,
    SCHEMA_VERSION,
    ArchiveEntry,
    Session,
    Verification,
    action_lock,
    ensure_directory,
    object_record,
    read_manifest,
    read_receipt,
    read_session,
    sync_directory,
    write_json_atomic,
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
        session_id,
        batch.card,
        batch.inventory_id,
        timestamp,
        destination,
        "copying",
        (),
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
        failed = replace(
            active,
            status="failed",
            failure_detail=str(error),
            notification_receipt=None,
        )
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
            raise ActionError(
                f"refusing mismatching/unrecorded completed file: {photo.relative_path!r}"
            )
        if file_digest(source) != (prior.size, prior.sha256):
            raise ActionError(f"source bytes changed: {photo.relative_path!r}")
        return session
    ensure_directory(destination.parent)
    path_id = hashlib.sha256(photo.relative_path.encode()).hexdigest()
    temporary = checked_path(
        destination.parent / f".uncharles-{session.session_id}-{path_id}.part"
    )
    digest = hashlib.sha256()
    size = 0
    descriptor = os.open(
        temporary,
        os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW | os.O_NONBLOCK,
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
        if (after.st_size, after.st_mtime_ns) != (
            photo.size,
            photo.mtime_ns,
        ) or size != photo.size:
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


NOTIFICATION_TITLE = "Photo import complete"
NOTIFICATION_PROGRAM = """on run argv
    display notification (item 2 of argv) with title (item 1 of argv)
end run"""


def submit_notification(settings: Settings, proof: Verification) -> None:
    """Submit text as data; synthetic mode never invokes desktop APIs."""
    body = f"Copied and verified {proof.count} photos to {proof.destination}"
    if settings.test_root is not None:
        sink = checked_path(settings.test_root / "notifications.jsonl")
        descriptor = os.open(
            sink,
            os.O_WRONLY | os.O_APPEND | os.O_CREAT | os.O_NOFOLLOW | os.O_NONBLOCK,
            0o600,
        )
        try:
            if not stat.S_ISREG(os.fstat(descriptor).st_mode):
                raise ActionError("mock notification sink is not a regular file")
            stream = os.fdopen(descriptor, "w", encoding="utf-8")
        except (OSError, ActionError):
            os.close(descriptor)
            raise
        with stream:
            stream.write(compact({"title": NOTIFICATION_TITLE, "body": body}) + "\n")
            stream.flush()
            os.fsync(stream.fileno())
        return
    if sys.platform != "darwin":
        raise ActionError("desktop notification requires macOS")
    subprocess.run(
        ["osascript", "-e", NOTIFICATION_PROGRAM, NOTIFICATION_TITLE, body],
        capture_output=True,
        check=True,
        timeout=10,
    )


def notify_verified(
    settings: Settings,
    captured_batch: dict[str, object],
    captured_verification: dict[str, object],
) -> None:
    """Revalidate sensor authorization before one notification submission."""
    current_batch(settings, captured_batch)
    with action_lock(settings):
        batch = current_batch(settings, captured_batch)
        session = read_session(settings)
        if session is None or not batch_matches(batch, session):
            raise ActionError("notification has no matching active session")
        captured = Verification.from_record(captured_verification)
        if (
            captured.session_id != session.session_id
            or captured.inventory_id != session.inventory_id
            or captured.destination != session.destination
        ):
            raise ActionError("stale notification belongs to another session")
        if session.status != "copied":
            raise ActionError("notification requires a completed, nonfailed session")
        try:
            receipt = read_receipt(session)
            if receipt is not None:
                if compact(captured_verification) != compact(receipt.record()):
                    raise ActionError("captured proof differs from saved submission")
                if session.notification_receipt is None:
                    write_session_atomic(
                        settings,
                        replace(
                            session,
                            notification_receipt=session.destination / RECEIPT_NAME,
                        ),
                    )
                return
            proof = verify_archive(batch, session)
            if compact(captured_verification) != compact(proof.record()):
                raise ActionError("captured verification proof is no longer valid")
            current_batch(settings, captured_batch)
            submit_notification(settings, proof)
            receipt_path = session.destination / RECEIPT_NAME
            write_json_atomic(
                receipt_path,
                {
                    "version": SCHEMA_VERSION,
                    "submitted": True,
                    "proof": proof.record(),
                    "title": NOTIFICATION_TITLE,
                    "body": f"Copied and verified {proof.count} photos to {proof.destination}",
                },
                immutable=True,
            )
            write_session_atomic(
                settings, replace(session, notification_receipt=receipt_path)
            )
        except (
            ActionError,
            ObservationError,
            OSError,
            subprocess.SubprocessError,
        ) as error:
            mark_failed(settings, session, error)
            raise


def reset_session(settings: Settings, expected_session_id: str) -> None:
    """Remove only the token-matching active record; retain every archive."""
    if not expected_session_id:
        raise ActionError("reset requires an expected session ID")
    session = read_session(settings)
    if session is None:
        return
    with action_lock(settings):
        session = read_session(settings)
        if session is None:
            return
        if session.session_id != expected_session_id:
            raise ActionError("stale reset token; active bookkeeping preserved")
        (settings.state_dir / ACTIVE_NAME).unlink()
        sync_directory(settings.state_dir)


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
    parser.add_argument("action", choices=("copy", "notify", "reset-session"))
    parser.add_argument(
        "--session-id", help="expected active session token for manual reset"
    )
    args = parser.parse_args()
    try:
        settings = Settings.from_env()
        if args.action == "copy":
            copy_batch(settings, captured_env("UNCHARLES_FACT_PHOTO_BATCH"))
        elif args.action == "notify":
            notify_verified(
                settings,
                captured_env("UNCHARLES_FACT_PHOTO_BATCH"),
                captured_env("UNCHARLES_FACT_ARCHIVE_VERIFIED"),
            )
        else:
            expected = args.session_id or os.environ.get(
                "UNCHARLES_FACT_SESSION_OBSOLETE", ""
            )
            reset_session(settings, expected)
        return 0
    except (
        ActionError,
        ObservationError,
        OSError,
        subprocess.SubprocessError,
    ) as error:
        print(f"action error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
