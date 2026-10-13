"""Version-one session records and read-only completion observations."""

from __future__ import annotations

import fcntl
import json
import os
import re
import stat
import tempfile
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass, replace
from pathlib import Path
from typing import Literal

from common import (
    MANIFEST_NAME,
    RECEIPT_NAME,
    Card,
    ObservationError,
    Settings,
    checked_path,
    compact,
    open_regular,
    safe_relative,
    validate_boundaries,
)

SCHEMA_VERSION = 1
ACTIVE_NAME = "active-session.json"


def object_record(value: object) -> dict[str, object]:
    """Require a JSON object at a record boundary."""
    if not isinstance(value, dict):
        raise ObservationError("record must be a JSON object")
    return value


def text(record: dict[str, object], key: str) -> str:
    """Require a nonempty string field."""
    value = record.get(key)
    if not isinstance(value, str) or not value:
        raise ObservationError(f"invalid string field: {key}")
    return value


def number(record: dict[str, object], key: str) -> int:
    """Require a nonnegative integer, excluding JSON booleans."""
    value = record.get(key)
    if type(value) is not int or value < 0:
        raise ObservationError(f"invalid integer field: {key}")
    return value


def digest_field(record: dict[str, object], key: str) -> str:
    """Require a lowercase SHA-256 digest."""
    value = text(record, key)
    if re.fullmatch(r"[0-9a-f]{64}", value) is None:
        raise ObservationError(f"invalid digest: {key}")
    return value


def read_json(path: Path) -> dict[str, object]:
    """Read a regular JSON record without following symlinks."""
    try:
        with open_regular(path) as stream:
            return object_record(json.load(stream))
    except (json.JSONDecodeError, UnicodeError) as error:
        raise ObservationError(f"invalid JSON: {path}") from error


@dataclass(frozen=True)
class ArchiveEntry:
    """Recorded size/hash of a completed owned photograph."""

    relative_path: str
    size: int
    sha256: str

    def record(self) -> dict[str, object]:
        """Serialize a per-file progress entry."""
        return {
            "relative_path": self.relative_path,
            "size": self.size,
            "sha256": self.sha256,
        }

    @classmethod
    def from_record(cls, value: object) -> ArchiveEntry:
        """Validate a saved photo entry."""
        record = object_record(value)
        relative = text(record, "relative_path")
        safe_relative(relative)
        return cls(relative, number(record, "size"), digest_field(record, "sha256"))


@dataclass(frozen=True)
class Verification:
    """Stable sensor proof bound to immutable copy evidence."""

    session_id: str
    inventory_id: str
    destination: Path
    manifest_sha256: str
    count: int

    def record(self) -> dict[str, object]:
        """Serialize the compact captured proof."""
        return {
            "session_id": self.session_id,
            "inventory_id": self.inventory_id,
            "destination": str(self.destination),
            "manifest_sha256": self.manifest_sha256,
            "count": self.count,
        }

    @classmethod
    def from_record(cls, value: object) -> Verification:
        """Validate a captured or receipted proof."""
        record = object_record(value)
        return cls(
            text(record, "session_id"),
            digest_field(record, "inventory_id"),
            checked_path(Path(text(record, "destination"))),
            digest_field(record, "manifest_sha256"),
            number(record, "count"),
        )


@dataclass(frozen=True)
class Session:
    """Persisted active import identity, progress, and action failure."""

    session_id: str
    card: Card
    inventory_id: str
    extraction_timestamp: str
    destination: Path
    status: Literal["copying", "copied", "failed"]
    files: tuple[ArchiveEntry, ...]
    failure_detail: str | None = None
    notification_receipt: Path | None = None

    def without_receipt(self) -> Session:
        """Return immutable copy evidence, separate from notification state."""
        return replace(self, notification_receipt=None)

    def record(self) -> dict[str, object]:
        """Serialize schema version one."""
        return {
            "version": SCHEMA_VERSION,
            "session_id": self.session_id,
            "source": {
                "volume_uuid": self.card.volume_uuid,
                "mount_path": str(self.card.mount_path),
                "device_id": self.card.device_id,
            },
            "inventory_id": self.inventory_id,
            "extraction_timestamp": self.extraction_timestamp,
            "destination": str(self.destination),
            "status": self.status,
            "files": [entry.record() for entry in self.files],
            "failure_detail": self.failure_detail,
            "notification_receipt": (
                str(self.notification_receipt) if self.notification_receipt else None
            ),
        }

    @classmethod
    def from_record(cls, value: object) -> Session:
        """Validate saved schema, identity, paths, and progress fields."""
        record = object_record(value)
        if (
            type(record.get("version")) is not int
            or record["version"] != SCHEMA_VERSION
        ):
            raise ObservationError("unsupported session schema version")
        session_id = text(record, "session_id")
        if re.fullmatch(r"[0-9a-f]{32}", session_id) is None:
            raise ObservationError("invalid session ID")
        source = object_record(record.get("source"))
        card = Card(
            text(source, "volume_uuid"),
            checked_path(Path(text(source, "mount_path"))),
            text(source, "device_id"),
        )
        timestamp = text(record, "extraction_timestamp")
        if re.fullmatch(r"\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}", timestamp) is None:
            raise ObservationError("invalid extraction timestamp")
        destination = checked_path(Path(text(record, "destination")))
        if destination.name != f"{timestamp}-{session_id}":
            raise ObservationError("destination name does not match session")
        status = record.get("status")
        if not isinstance(status, str) or status not in {"copying", "copied", "failed"}:
            raise ObservationError("invalid session status")
        values = record.get("files")
        if not isinstance(values, list):
            raise ObservationError("session files must be a list")
        files = tuple(ArchiveEntry.from_record(entry) for entry in values)
        names = [entry.relative_path for entry in files]
        if len(names) != len(set(names)) or names != sorted(names):
            raise ObservationError("duplicate or unordered manifest paths")
        failure = record.get("failure_detail")
        if failure is not None and not isinstance(failure, str):
            raise ObservationError("invalid failure detail")
        if (status == "failed") != bool(failure):
            raise ObservationError("failure detail/status mismatch")
        receipt_value = record.get("notification_receipt")
        receipt = None
        if receipt_value is not None:
            if not isinstance(receipt_value, str):
                raise ObservationError("invalid receipt path")
            receipt = checked_path(Path(receipt_value))
            if receipt != destination / RECEIPT_NAME or status != "copied":
                raise ObservationError("receipt does not belong to copied session")
        return cls(
            session_id,
            card,
            digest_field(record, "inventory_id"),
            timestamp,
            destination,
            status,
            files,
            failure,
            receipt,
        )


def validate_session(settings: Settings, session: Session) -> None:
    """Constrain persisted destinations before observation or mutation."""
    validate_boundaries(settings, session.card.mount_path)
    if session.destination.parent != checked_path(settings.pictures_dir):
        raise ObservationError("saved archive is outside configured Pictures")
    checked_path(session.destination)


def read_session(settings: Settings) -> Session | None:
    """Load active state read-only; only genuine absence means no session."""
    path = checked_path(settings.state_dir / ACTIVE_NAME)
    try:
        record = read_json(path)
    except FileNotFoundError:
        return None
    session = Session.from_record(record)
    validate_session(settings, session)
    return session


def read_manifest(destination: Path) -> Session:
    """Read the immutable completed copy manifest."""
    session = Session.from_record(read_json(destination / MANIFEST_NAME))
    if (
        session.destination != destination
        or session.status != "copied"
        or session.notification_receipt is not None
    ):
        raise ObservationError("invalid completed copy manifest")
    return session


def read_receipt(session: Session) -> Verification | None:
    """Observe a separate successful submission receipt, if present."""
    path = session.destination / RECEIPT_NAME
    try:
        record = read_json(path)
    except FileNotFoundError:
        if session.notification_receipt is not None:
            raise ObservationError("referenced notification receipt is missing")
        return None
    if (
        type(record.get("version")) is not int
        or record["version"] != SCHEMA_VERSION
        or record.get("submitted") is not True
    ):
        raise ObservationError("invalid notification receipt schema")
    proof = Verification.from_record(record.get("proof"))
    if (
        proof.session_id != session.session_id
        or proof.inventory_id != session.inventory_id
        or proof.destination != session.destination
        or proof.count != len(session.files)
    ):
        raise ObservationError("notification receipt identity mismatch")
    from common import file_digest

    if proof.manifest_sha256 != file_digest(session.destination / MANIFEST_NAME)[1]:
        raise ObservationError("notification receipt manifest mismatch")
    return proof


# These helpers are called only by actions; readers never acquire/create locks.
def ensure_directory(path: Path) -> None:
    """Create an action-owned directory tree, rejecting symlink components."""
    checked_path(path)
    path.mkdir(parents=True, exist_ok=True)
    checked_path(path)
    if not path.is_dir():
        raise ObservationError(f"not a directory: {path}")


def sync_directory(path: Path) -> None:
    """Flush publication/removal metadata after a durable file write."""
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def write_json_atomic(
    path: Path, record: dict[str, object], *, immutable: bool = False
) -> None:
    """Publish durable JSON; immutable records never replace existing bytes."""
    checked_path(path)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temp = Path(temporary)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
            stream.write(compact(record) + "\n")
            stream.flush()
            os.fsync(stream.fileno())
        if immutable:
            # Link/unlink is an atomic no-clobber rename equivalent on this
            # filesystem; plain rename would overwrite an unrelated file.
            os.link(temp, path, follow_symlinks=False)
        else:
            os.replace(temp, path)
        sync_directory(path.parent)
    finally:
        temp.unlink(missing_ok=True)


def write_session_atomic(settings: Settings, session: Session) -> None:
    """Persist active progress while the caller holds the action lock."""
    validate_session(settings, session)
    write_json_atomic(settings.state_dir / ACTIVE_NAME, session.record())


@contextmanager
def action_lock(settings: Settings) -> Iterator[None]:
    """Serialize action writers; acquiring a lock is itself a mutation."""
    ensure_directory(settings.state_dir)
    path = checked_path(settings.state_dir / "action.lock")
    descriptor = os.open(path, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    try:
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise ObservationError("action lock is not a regular file")
        fcntl.flock(descriptor, fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(descriptor, fcntl.LOCK_UN)
    finally:
        os.close(descriptor)
