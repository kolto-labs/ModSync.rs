"""Retain narrowly pinned original INI/NSS inputs inside encrypted lane evidence."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import stat
import subprocess
import tempfile
import time
import zipfile
import zlib


LIMIT = 2 * 1024 * 1024
TOTAL = 8 * 1024 * 1024


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def regular(path):
    if not stat.S_ISREG(path.lstat().st_mode):
        raise ValueError('Source diagnostic input is not a regular file')


def capture_command(command, maximum):
    with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as errors:
        environment = {name: os.environ[name] for name in
                       ('PATH', 'LANG', 'LC_ALL', 'TZ', 'TMPDIR', 'TEMP', 'SYSTEMROOT', 'WINDIR')
                       if name in os.environ}
        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=output, stderr=errors,
                                   env=environment)
        deadline = time.monotonic() + 120
        try:
            while process.poll() is None:
                if time.monotonic() > deadline or os.fstat(output.fileno()).st_size > maximum or os.fstat(errors.fileno()).st_size > LIMIT:
                    raise ValueError('Source diagnostic reader exceeded its bound')
                time.sleep(0.05)
            if process.returncode:
                raise ValueError('Source diagnostic archive reader failed')
            if os.fstat(output.fileno()).st_size > maximum:
                raise ValueError('Source diagnostic member exceeded its bound')
            output.seek(0)
            return output.read(maximum + 1)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()


def member_name(name):
    value = name.replace('\\', '/')
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in ('', '.', '..') for part in value.split('/')) or ':' in value or '\x00' in value:
        raise ValueError('Source diagnostic archive path is unsafe')
    return value


def members(archive):
    if zipfile.is_zipfile(archive):
        with zipfile.ZipFile(archive) as source:
            infos = source.infolist()
            if len(infos) > 10000:
                raise ValueError('Source diagnostic archive has too many members')
            seen = set()
            for info in infos:
                if info.is_dir():
                    continue
                name = member_name(info.filename)
                if name in seen:
                    raise ValueError('Duplicate source diagnostic archive path')
                seen.add(name)
                if PurePosixPath(name).suffix.lower() in ('.ini', '.nss'):
                    if info.file_size > LIMIT or stat.S_IFMT(info.external_attr >> 16) not in (0, stat.S_IFREG):
                        raise ValueError('Source diagnostic member kind or size differs')
                    with source.open(info) as stream:
                        data = stream.read(LIMIT + 1)
                    yield name, data, info.CRC
        return
    listing = capture_command(['7z', 'l', '-slt', '-ba', '--', str(archive)], LIMIT).decode('utf-8', 'strict')
    rows = listing.strip().split('\n\n')
    if len(rows) > 10000:
        raise ValueError('Source diagnostic archive has too many members')
    seen = set()
    for row in rows:
        fields = dict(line.split(' = ', 1) for line in row.splitlines() if ' = ' in line)
        if fields.get('Folder') == '+':
            continue
        name = member_name(fields['Path'])
        if name in seen:
            raise ValueError('Duplicate source diagnostic archive path')
        seen.add(name)
        if PurePosixPath(name).suffix.lower() not in ('.ini', '.nss'):
            continue
        size = int(fields['Size'])
        if size > LIMIT or any(key in fields for key in ('Symbolic Link', 'Hard Link')):
            raise ValueError('Source diagnostic member kind or size differs')
        command = (['unrar', 'p', '-inul', '-p-', str(archive), fields['Path']]
                   if archive.suffix.lower() == '.rar' else
                   ['7z', 'x', '-so', '-y', '-spd', '--', str(archive), fields['Path']])
        data = capture_command(command, LIMIT)
        if len(data) != size:
            raise ValueError('Source diagnostic archive member length differs')
        crc = int(fields['CRC'], 16) if fields.get('CRC') else None
        yield name, data, crc


def retain(archives, destination):
    """Diagnostic failures are recorded alongside the primary installation result."""
    archives, destination = Path(archives), Path(destination)
    policy = json.loads(Path(__file__).with_suffix('.json').read_text())
    destination.mkdir(mode=0o700)
    report = {'schema': 1, 'purpose': 'source-pinned original text for compiler diagnosis', 'archives': []}
    for request in policy['entries']:
        row = {'component_guid': request['component_guid'], 'archive_sha256': request['sha256'], 'members': [], 'status': 'failed'}
        report['archives'].append(row)
        try:
            archive = archives / request['filename']
            regular(archive)
            if archive.stat().st_size != request['archive_bytes'] or digest(archive) != request['sha256']:
                raise ValueError('Source diagnostic whole archive identity differs')
            target = destination / request['sha256']
            target.mkdir(mode=0o700)
            total = 0
            for name, data, crc in members(archive):
                total += len(data)
                if len(data) > LIMIT or total > TOTAL:
                    raise ValueError('Source diagnostic text budget exceeded')
                actual_crc = zlib.crc32(data) & 0xffffffff
                if crc is not None and crc != actual_crc:
                    raise ValueError('Source diagnostic archive CRC differs')
                path = target / name
                path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
                with path.open('xb') as stream:
                    os.chmod(path, 0o600)
                    stream.write(data)
                row['members'].append({'path': name, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest(), 'crc32': f'{actual_crc:08x}'})
            if not row['members'] or digest(archive) != request['sha256']:
                raise ValueError('Source diagnostic archive changed or contains no selected text')
            row['status'] = 'verified-source-text-retained'
        except (OSError, ValueError, KeyError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
            row['failure_category'] = type(error).__name__
    (destination / 'manifest.json').write_text(json.dumps(report, indent=2) + '\n')
    return report
