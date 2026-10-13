"""Identity-bound, source-preserving photo archiver mutations."""

import argparse
import hashlib
import json
import os
import stat
import subprocess
import sys
import uuid
from contextlib import ExitStack, contextmanager
from dataclasses import replace
from datetime import datetime
from pathlib import Path
from typing import Final

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

PRIVATE_FILE_MODE: Final = 0o600
NOTIFICATION_TIMEOUT_SECONDS: Final = 10
NOTIFICATION_TITLE: Final = "Photo import complete"
NOTIFICATION_PROGRAM: Final = """on run argv
    display notification (item 2 of argv) with title (item 1 of argv)
end run"""


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
    destination = checked_path(
        settings.pictures_dir / f"{timestamp}-{session_id}"
    )
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


def mark_failed(
    settings: Settings, session: Session, error: BaseException
) -> None:
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


from collections.abc import Generator


@contextmanager
def _open_output(
    path: Path, *, description: str, append: bool = False
) -> Generator:
    """Open regular binary output, releasing its descriptor on every exit.

    Append to notification sinks; truncate owned copy temporaries.
    The stack owns the descriptor even if stream construction fails.
    """
    flags = os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW | os.O_NONBLOCK
    if append:
        flags |= os.O_APPEND
    descriptor = os.open(path, flags, PRIVATE_FILE_MODE)
    with ExitStack() as resources:
        resources.callback(os.close, descriptor)
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise ActionError(f"{description} is not a regular file")
        if not append:
            os.ftruncate(descriptor, 0)
        output = resources.enter_context(
            os.fdopen(descriptor, "ab" if append else "wb", closefd=False)
        )
        yield output



def _verify_copied_photo(
    photo: Photo, source: Path, destination: Path, prior: ArchiveEntry | None
) -> None:
    """Reject unjournaled or changed files instead of overwriting them."""
    if prior is None or file_digest(destination) != (prior.size, prior.sha256):
        raise ActionError(
            "refusing mismatching/unrecorded completed file: "
            f"{photo.relative_path!r}"
        )
    if file_digest(source) != (prior.size, prior.sha256):
        raise ActionError(f"source bytes changed: {photo.relative_path!r}")


def _copy_to_temporary(
    photo: Photo, source: Path, temporary: Path
) -> ArchiveEntry:
    """Stream, hash, and flush bytes while checking source metadata."""
    digest = hashlib.sha256()
    size = 0
    metadata = (photo.size, photo.mtime_ns)
    with (
        _open_output(temporary, description="owned temporary") as output,
        open_regular(source) as stream,
    ):
        before = os.fstat(stream.fileno())
        if (before.st_size, before.st_mtime_ns) != metadata:
            raise ActionError("source metadata changed before copying")
        while chunk := stream.read(CHUNK_SIZE):
            output.write(chunk)
            digest.update(chunk)
            size += len(chunk)
        after = os.fstat(stream.fileno())
        after_metadata = (after.st_size, after.st_mtime_ns)
        if after_metadata != metadata or size != photo.size:
            raise ActionError("source metadata changed during copying")
        output.flush()
        os.fsync(output.fileno())
    return ArchiveEntry(photo.relative_path, size, digest.hexdigest())


def copy_photo(settings: Settings, session: Session, photo: Photo) -> Session:
    """Stream an owned temporary; journal its hash before publication."""
    relative = safe_relative(photo.relative_path)
    source = session.card.mount_path / relative
    destination = checked_path(session.destination / relative)
    recorded = {entry.relative_path: entry for entry in session.files}
    prior = recorded.get(photo.relative_path)
    if destination.exists():
        _verify_copied_photo(photo, source, destination, prior)
        return session
    ensure_directory(destination.parent)
    path_id = hashlib.sha256(photo.relative_path.encode()).hexdigest()
    temporary = checked_path(
        destination.parent / f".uncharles-{session.session_id}-{path_id}.part"
    )
    entry = _copy_to_temporary(photo, source, temporary)
    if prior is not None and prior != entry:
        raise ActionError(
            "source no longer matches previously journaled bytes"
        )
    recorded[photo.relative_path] = entry
    session = replace(
        session, files=tuple(recorded[key] for key in sorted(recorded))
    )
    # Write-ahead progress reconciles a crash on either side of publication:
    # a missing final is retried; a present final must match this saved hash.
    write_session_atomic(settings, session)
    os.link(temporary, destination, follow_symlinks=False)
    sync_directory(destination.parent)
    temporary.unlink()
    sync_directory(destination.parent)
    return session


def complete_copy(
    settings: Settings, batch: Batch, session: Session
) -> Session:
    """Finish immutable evidence only after all progress/source checks."""
    current_batch(settings, batch.capture())
    expected = {photo.relative_path for photo in batch.files}
    if {entry.relative_path for entry in session.files} != expected:
        raise ActionError(
            "copy progress does not cover the complete inventory"
        )
    completed = replace(session, status="copied", failure_detail=None)
    manifest_path = session.destination / MANIFEST_NAME
    if manifest_path.exists():
        if read_manifest(session.destination) != completed:
            raise ActionError(
                "existing completed manifest differs from session"
            )
    else:
        write_json_atomic(manifest_path, completed.record(), immutable=True)
    write_session_atomic(settings, completed)
    return completed


def copy_batch(
    settings: Settings, captured_batch: dict[str, object]
) -> Session:
    """Copy or resume only a matching batch, under the action writer lock."""
    current_batch(settings, captured_batch)
    with action_lock(settings):
        batch = current_batch(settings, captured_batch)
        session = read_session(settings)
        if session is not None and not batch_matches(batch, session):
            raise ActionError("obsolete session must be reset before copying")
        if session is not None and session.status == "failed":
            raise ActionError(
                "failed session requires explicit operator recovery"
            )
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


def _notification_body(proof: Verification) -> str:
    """Use identical completion text for submission and its receipt."""
    return f"Copied and verified {proof.count} photos to {proof.destination}"


def _notification_session(
    batch: Batch,
    session: Session | None,
    captured_verification: dict[str, object],
) -> Session:
    """Require a current completed session before handling its proof."""
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
        raise ActionError(
            "notification requires a completed, nonfailed session"
        )
    return session


def _restore_notification_receipt(
    settings: Settings,
    session: Session,
    captured_verification: dict[str, object],
    receipt: Verification,
) -> None:
    """Recover active bookkeeping without resubmitting a notification."""
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


def _record_notification(
    settings: Settings, session: Session, proof: Verification
) -> None:
    """Publish the immutable receipt before updating active bookkeeping."""
    receipt_path = session.destination / RECEIPT_NAME
    write_json_atomic(
        receipt_path,
        {
            "version": SCHEMA_VERSION,
            "submitted": True,
            "proof": proof.record(),
            "title": NOTIFICATION_TITLE,
            "body": _notification_body(proof),
        },
        immutable=True,
    )
    write_session_atomic(
        settings, replace(session, notification_receipt=receipt_path)
    )


def submit_notification(settings: Settings, proof: Verification) -> None:
    """Submit text as data; synthetic mode never invokes desktop APIs."""
    body = _notification_body(proof)
    if settings.test_root is not None:
        sink = checked_path(settings.test_root / "notifications.jsonl")
        with _open_output(
            sink, description="mock notification sink", append=True
        ) as stream:
            record = compact({"title": NOTIFICATION_TITLE, "body": body})
            stream.write((record + "\n").encode("utf-8"))
            stream.flush()
            os.fsync(stream.fileno())
        return
    if sys.platform != "darwin":
        raise ActionError("desktop notification requires macOS")
    subprocess.run(
        ["osascript", "-e", NOTIFICATION_PROGRAM, NOTIFICATION_TITLE, body],
        capture_output=True,
        check=True,
        timeout=NOTIFICATION_TIMEOUT_SECONDS,
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
        session = _notification_session(
            batch, read_session(settings), captured_verification
        )
        try:
            receipt = read_receipt(session)
            if receipt is not None:
                _restore_notification_receipt(
                    settings, session, captured_verification, receipt
                )
                return
            proof = verify_archive(batch, session)
            if compact(captured_verification) != compact(proof.record()):
                raise ActionError(
                    "captured verification proof is no longer valid"
                )
            current_batch(settings, captured_batch)
            submit_notification(settings, proof)
            _record_notification(settings, session, proof)
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
            raise ActionError(
                "stale reset token; active bookkeeping preserved"
            )
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
        match args.action:
            case "copy":
                copy_batch(
                    settings, captured_env("UNCHARLES_FACT_PHOTO_BATCH")
                )
            case "notify":
                notify_verified(
                    settings,
                    captured_env("UNCHARLES_FACT_PHOTO_BATCH"),
                    captured_env("UNCHARLES_FACT_ARCHIVE_VERIFIED"),
                )
            case "reset-session":
                expected = args.session_id or os.environ.get(
                    "UNCHARLES_FACT_SESSION_OBSOLETE", ""
                )
                reset_session(settings, expected)
            case _:
                raise ActionError(f"unknown action: {args.action}")
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
