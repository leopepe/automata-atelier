"""Read-only settings, card discovery, inventory, and archive integrity."""

from __future__ import annotations

import hashlib
import json
import os
import plistlib
import stat
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, BinaryIO

if TYPE_CHECKING:
    from session import Session, Verification

EXTENSIONS = frozenset({".jpg", ".jpeg", ".cr2", ".cr3", ".crw"})
CHUNK_SIZE = 1024 * 1024
MANIFEST_NAME = ".uncharles-photo-archive.json"
RECEIPT_NAME = ".uncharles-photo-notification.json"


class ObservationError(ValueError):
    """An observation or configuration cannot be trusted."""


class IntegrityError(ObservationError):
    """An observed archive does not match its source and manifest."""


def require_python() -> None:
    """Reject unsupported interpreters before helper I/O."""
    if sys.version_info < (3, 14):
        raise ObservationError("Python 3.14 or newer is required")


def compact(value: object) -> str:
    """Encode stable machine data, including arbitrary filename text."""
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def checked_path(path: Path) -> Path:
    """Normalize an absolute path and reject existing symlink components."""
    path = Path(os.path.abspath(path.expanduser()))
    for component in [*reversed(path.parents), path]:
        if component.is_symlink():
            raise ObservationError(f"symlink path is not allowed: {component}")
    return path


def overlaps(left: Path, right: Path) -> bool:
    """Return whether either normalized tree contains the other."""
    return left.is_relative_to(right) or right.is_relative_to(left)


@dataclass(frozen=True)
class Settings:
    """Operator paths, with an explicitly confined synthetic mode."""

    pictures_dir: Path
    state_dir: Path
    selected_volume_uuid: str | None
    test_root: Path | None

    @classmethod
    def from_env(cls) -> Settings:
        """Read settings without creating or provisioning any paths."""
        require_python()
        root_value = os.environ.get("PHOTO_ARCHIVER_TEST_ROOT")
        if root_value is not None:
            if not root_value.strip():
                raise ObservationError("PHOTO_ARCHIVER_TEST_ROOT is empty")
            root = checked_path(Path(root_value).expanduser().resolve())
            if root == Path(root.anchor):
                raise ObservationError("test root cannot be a filesystem root")
            return cls(root / "Pictures", root / "state", None, root)
        pictures = checked_path(
            Path(
                os.environ.get(
                    "PHOTO_ARCHIVER_PICTURES_DIR",
                    str(Path.home() / "Pictures"),
                )
            )
            .expanduser()
            .resolve()
        )
        state = checked_path(
            Path(
                os.environ.get(
                    "PHOTO_ARCHIVER_STATE_DIR",
                    str(
                        Path.home()
                        / "Library/Application Support/uncharles-photo-archiver"
                    ),
                )
            )
            .expanduser()
            .resolve()
        )
        if overlaps(pictures, state):
            raise ObservationError("Pictures and state trees must be separate")
        return cls(pictures, state, os.environ.get("PHOTO_ARCHIVER_VOLUME_UUID"), None)


def validate_boundaries(settings: Settings, source: Path) -> None:
    """Reject output/state overlap with each other or the selected source."""
    source = checked_path(source)
    pictures = checked_path(settings.pictures_dir)
    state = checked_path(settings.state_dir)
    if (
        overlaps(source, pictures)
        or overlaps(source, state)
        or overlaps(pictures, state)
    ):
        raise ObservationError("source, Pictures, and state trees must be separate")
    if settings.test_root is not None and source != settings.test_root / "card":
        raise ObservationError("synthetic source escaped test root")


@dataclass(frozen=True)
class Card:
    """A selected mounted volume with stable volume identity."""

    volume_uuid: str
    mount_path: Path
    device_id: str


@dataclass(frozen=True)
class Photo:
    """Source metadata used only for inventory identity, not integrity."""

    relative_path: str
    size: int
    mtime_ns: int


@dataclass(frozen=True)
class Batch:
    """Deterministically ordered supported source photographs."""

    card: Card
    files: tuple[Photo, ...]
    inventory_id: str
    total_bytes: int

    def capture(self) -> dict[str, object]:
        """Return the compact action guard, without filename inventories."""
        return {
            "volume_uuid": self.card.volume_uuid,
            "mount_path": str(self.card.mount_path),
            "inventory_id": self.inventory_id,
            "count": len(self.files),
            "total_bytes": self.total_bytes,
        }


def diskutil(*args: str) -> dict[str, object]:
    """Read a diskutil plist with a bounded subprocess timeout."""
    result = subprocess.run(
        ["diskutil", *args],
        capture_output=True,
        check=True,
        timeout=10,
    )
    try:
        value = plistlib.loads(result.stdout)
    except (plistlib.InvalidFileException, ValueError) as error:
        raise ObservationError("invalid diskutil plist") from error
    if not isinstance(value, dict):
        raise ObservationError("diskutil plist must be a dictionary")
    return value


def discover_card(settings: Settings) -> Card | None:
    """Observe one qualifying physical card; uncertainty is an error."""
    if settings.test_root is not None:
        mount = checked_path(settings.test_root / "card")
        if not mount.exists():
            return None
        if not mount.is_dir():
            raise ObservationError("synthetic card is not a directory")
        validate_boundaries(settings, mount)
        identity = hashlib.sha256(str(settings.test_root).encode()).hexdigest()
        return Card(f"mock-{identity}", mount, "mock-card")
    if sys.platform != "darwin":
        raise ObservationError("real discovery requires macOS; use test root")
    listing = diskutil("list", "-plist", "physical")
    disks = listing.get("AllDisksAndPartitions")
    if not isinstance(disks, list):
        raise ObservationError("physical disk list is missing")
    candidates: list[Card] = []
    for disk in disks:
        if not isinstance(disk, dict) or not isinstance(
            disk.get("DeviceIdentifier"), str
        ):
            raise ObservationError("physical disk identity is missing")
        parent = diskutil("info", "-plist", disk["DeviceIdentifier"])
        partitions = disk.get("Partitions", [])
        if not isinstance(partitions, list):
            raise ObservationError("partition list is invalid")
        for partition in partitions:
            if not isinstance(partition, dict):
                raise ObservationError("partition identity is invalid")
            device = partition.get("DeviceIdentifier")
            if not isinstance(device, str):
                raise ObservationError("partition device is missing")
            info = diskutil("info", "-plist", device)
            mount_value = info.get("MountPoint")
            if not mount_value or info.get("Mounted") is False:
                continue
            if not isinstance(mount_value, str):
                raise ObservationError("mount path is invalid")
            if mount_value == "/" or info.get("OSInternalMedia") is True:
                continue
            removable = parent.get("RemovableMedia")
            ejectable = parent.get("Ejectable")
            if not isinstance(removable, bool) or not isinstance(ejectable, bool):
                raise ObservationError(
                    "media RemovableMedia/Ejectable classification is missing or invalid"
                )
            if not (removable or ejectable):
                continue
            protocol = parent.get("BusProtocol")
            uuid = info.get("VolumeUUID")
            if settings.selected_volume_uuid is not None:
                if uuid != settings.selected_volume_uuid:
                    continue
                if protocol not in {"USB", "Secure Digital"}:
                    raise ObservationError("selected media classification is unknown")
            elif protocol != "Secure Digital":
                if protocol is None:
                    raise ObservationError(
                        "removable media bus classification is missing"
                    )
                continue
            if not isinstance(uuid, str) or not uuid:
                raise ObservationError("card volume UUID is missing")
            mount = checked_path(Path(mount_value))
            if not mount.is_dir():
                raise ObservationError("mounted card cannot be read")
            validate_boundaries(settings, mount)
            candidates.append(Card(uuid, mount, device))
    if len(candidates) > 1:
        raise ObservationError("ambiguous photo cards; select a volume UUID")
    return candidates[0] if candidates else None


def photo_files(root: Path) -> tuple[Photo, ...]:
    """Enumerate regular DCIM photos without following any symlinks."""
    root = checked_path(root)
    dcim = root / "DCIM"
    if dcim.is_symlink() or not dcim.exists():
        return ()
    if not dcim.is_dir():
        raise ObservationError("DCIM is not a directory")
    photos: list[Photo] = []

    def walk(directory: Path) -> None:
        with os.scandir(directory) as entries:
            for entry in entries:
                if entry.is_symlink():
                    continue
                path = Path(entry.path)
                if entry.is_dir(follow_symlinks=False):
                    walk(path)
                elif (
                    entry.is_file(follow_symlinks=False)
                    and path.suffix.lower() in EXTENSIONS
                ):
                    metadata = entry.stat(follow_symlinks=False)
                    photos.append(
                        Photo(
                            path.relative_to(root).as_posix(),
                            metadata.st_size,
                            metadata.st_mtime_ns,
                        )
                    )

    walk(dcim)
    return tuple(sorted(photos, key=lambda photo: photo.relative_path))


def scan_photos(card: Card) -> Batch:
    """Read a deterministic path/size/mtime inventory token."""
    files = photo_files(card.mount_path)
    metadata = [[photo.relative_path, photo.size, photo.mtime_ns] for photo in files]
    identity = hashlib.sha256(compact(metadata).encode()).hexdigest()
    return Batch(card, files, identity, sum(photo.size for photo in files))


def safe_relative(relative_path: str) -> Path:
    """Validate a canonical supported source-relative photo path."""
    path = Path(relative_path)
    if (
        path.is_absolute()
        or ".." in path.parts
        or len(path.parts) < 2
        or path.parts[0] != "DCIM"
        or path.as_posix() != relative_path
        or path.suffix.lower() not in EXTENSIONS
    ):
        raise ObservationError(f"invalid photo path: {relative_path!r}")
    return path


def open_regular(path: Path) -> BinaryIO:
    """Open a checked regular file without following its final symlink."""
    checked_path(path)
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise ObservationError(f"not a regular file: {path}")
        return os.fdopen(descriptor, "rb")
    except (OSError, ObservationError):
        os.close(descriptor)
        raise


def file_digest(path: Path) -> tuple[int, str]:
    """Hash a regular file with bounded memory."""
    digest = hashlib.sha256()
    size = 0
    with open_regular(path) as stream:
        while chunk := stream.read(CHUNK_SIZE):
            digest.update(chunk)
            size += len(chunk)
    return size, digest.hexdigest()


def batch_matches(batch: Batch, session: Session) -> bool:
    """Compare the complete source identity independently of captures."""
    return (
        batch.card.volume_uuid == session.card.volume_uuid
        and batch.card.mount_path == session.card.mount_path
        and batch.inventory_id == session.inventory_id
    )


def verify_archive(batch: Batch, session: Session) -> Verification:
    """Compare exact paths, sizes, hashes, and immutable manifest bytes."""
    from session import Verification, read_manifest

    if session.status != "copied" or not batch_matches(batch, session):
        raise IntegrityError("session is not a current completed copy")
    manifest = read_manifest(session.destination)
    if manifest != session.without_receipt():
        raise IntegrityError("manifest does not match completed session")
    expected = {photo.relative_path: photo for photo in batch.files}
    saved = {entry.relative_path: entry for entry in manifest.files}
    observed = {photo.relative_path for photo in photo_files(session.destination)}
    if set(expected) != set(saved) or set(saved) != observed:
        raise IntegrityError("source/manifest/destination photo path sets differ")
    for relative, entry in saved.items():
        source = file_digest(batch.card.mount_path / safe_relative(relative))
        destination = file_digest(session.destination / safe_relative(relative))
        if source != (entry.size, entry.sha256) or destination != source:
            raise IntegrityError(f"size/hash mismatch: {relative!r}")
        if expected[relative].size != entry.size:
            raise IntegrityError(f"inventory size mismatch: {relative!r}")
    if scan_photos(batch.card).capture() != batch.capture():
        raise IntegrityError("source inventory changed during verification")
    digest = file_digest(session.destination / MANIFEST_NAME)[1]
    return Verification(
        session.session_id,
        session.inventory_id,
        session.destination,
        digest,
        len(saved),
    )
