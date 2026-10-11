"""Offline QA inside a disposable Docker tools container; all inputs read-only.

Explicit -r for every tool invocation. No network capture or name resolution.
Checks independent binary records against tshark fields and external rewrites.
"""
import hashlib
import json
import struct
import subprocess
import sys
from decimal import Decimal
from pathlib import Path


def run(*args):
    result = subprocess.run(args, capture_output=True, text=True, timeout=180)
    if result.returncode:
        raise RuntimeError(f"{args[0]} failed: {result.stderr[:1024]}")
    return result.stdout


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def records(path):
    with path.open('rb') as stream:
        magic = stream.read(4)
        pcap = {
            b'\xd4\xc3\xb2\xa1': ('<', 1000000),
            b'\xa1\xb2\xc3\xd4': ('>', 1000000),
            b'\x4d\x3c\xb2\xa1': ('<', 1000000000),
            b'\xa1\xb2\x3c\x4d': ('>', 1000000000),
        }
        if magic in pcap:
            endian, resolution = pcap[magic]
            header = stream.read(20)
            assert len(header) == 20
            while prefix := stream.read(16):
                assert len(prefix) == 16
                seconds, ticks, captured, original = struct.unpack(endian+'IIII', prefix)
                assert 0 < captured <= original and captured <= 1048576
                payload = stream.read(captured)
                assert len(payload) == captured
                yield Decimal(seconds)+Decimal(ticks)/resolution, captured, original, payload
            return
        assert magic == b'\x0a\x0d\x0d\x0a', 'unknown capture format'
        stream.seek(0)
        interfaces = []
        endian = '<'
        while prefix := stream.read(8):
            assert len(prefix) == 8
            if prefix[:4] == b'\x0a\x0d\x0d\x0a':
                order = stream.read(4)
                endian = '<' if order == b'\x4d\x3c\x2b\x1a' else '>'
                length = struct.unpack(endian+'I', prefix[4:])[0]
                block = prefix+order+stream.read(length-12)
                interfaces = []
            else:
                length = struct.unpack(endian+'I', prefix[4:])[0]
                assert 12 <= length <= 4194304 and length % 4 == 0
                block = prefix+stream.read(length-8)
            assert len(block) == length and struct.unpack(endian+'I', block[-4:])[0] == length
            kind = struct.unpack(endian+'I', block[:4])[0]
            if kind == 1:
                resolution = 1000000
                offset = 0
                index = 16
                while index+4 <= length-4:
                    code, size = struct.unpack(endian+'HH', block[index:index+4])
                    if code == 0:
                        break
                    value = block[index+4:index+4+size]
                    if code == 9:
                        exponent = value[0]
                        resolution = 2**(exponent & 127) if exponent & 128 else 10**exponent
                    if code == 14:
                        offset = struct.unpack(endian+'q', value)[0]
                    index += 4+((size+3)//4)*4
                interfaces.append((resolution, offset))
            elif kind == 6:
                interface, high, low, captured, original = struct.unpack(endian+'IIIII', block[8:28])
                resolution, offset = interfaces[interface]
                payload = block[28:28+captured]
                assert len(payload) == captured and captured <= original
                yield Decimal(offset)+Decimal((high << 32)+low)/resolution, captured, original, payload


def digest(path):
    result = hashlib.sha256()
    count = clipped = 0
    for timestamp, captured, original, payload in records(path):
        count += 1
        clipped += captured < original
        result.update(f'{timestamp:.9f},{captured},{original}\n'.encode())
        result.update(payload)
    return dict(packets=count, clipped=clipped, record_sha256=result.hexdigest(), file_sha256=sha(path))


def inspect(path, target):
    before = sha(path)
    expected = digest(path)
    fields = run('tshark', '-n', '-r', str(path), '-T', 'fields', '-e', 'frame.number', '-e', 'frame.time_epoch', '-e', 'frame.cap_len', '-e', 'frame.len').splitlines()
    assert len(fields) == expected['packets']
    for number, ((timestamp, captured, original, _), line) in enumerate(zip(records(path), fields), 1):
        actual_number, actual_time, actual_captured, actual_original = line.split('\t')
        assert int(actual_number) == number and Decimal(actual_time) == timestamp
        assert int(actual_captured) == captured and int(actual_original) == original
    raw = json.loads(run('tshark', '-n', '-r', str(path), '-c', '3', '-T', 'jsonraw'))
    assert len(raw) == min(3, expected['packets'])
    for frame, (_, _, _, payload) in zip(raw, records(path)):
        assert bytes.fromhex(frame['_source']['layers']['frame_raw'][0]) == payload
    target.mkdir()
    pcapng = target/'tshark.pcapng'
    run('tshark', '-n', '-r', str(path), '-F', 'pcapng', '-w', str(pcapng))
    assert digest(pcapng)['record_sha256'] == expected['record_sha256']
    pcap = target/'editcap.pcap'
    run('editcap', '-F', 'nsecpcap', str(path), str(pcap))
    assert digest(pcap)['record_sha256'] == expected['record_sha256']
    tcpdump = target/'tcpdump.pcap'
    run('tcpdump', '-n', '-r', str(pcap), '--nano', '-w', str(tcpdump))
    assert digest(tcpdump)['record_sha256'] == expected['record_sha256']
    assert sha(path) == before
    return dict(source=str(path), **expected, tshark_all_timestamps_and_lengths=True, tshark_raw_sample=True, external_rewrites_byte_identical=True, original_preserved=True)


def main():
    output = Path('/out')
    assert not (output/'verification.json').exists()
    files = [Path('/fixtures')/name for name in ['baseline.pcap', 'deviation.pcap', 'pcapng/selected.pcapng']]
    files += sorted(Path('/exports').glob('*/selected.pcap*'))
    if len(sys.argv) > 1:
        files = [Path(sys.argv[1])]
    results = [inspect(path, output/f'capture-{index:02}') for index, path in enumerate(files, 1)]
    verification = dict(passed=True, inputs=results, versions={tool:run(tool, '--version').splitlines()[0] for tool in ['tshark', 'editcap', 'tcpdump']}, network_capture=False, all_inputs_read_only=True, raw_check_limit=3)
    (output/'verification.json').write_text(json.dumps(verification, indent=2))
    print(json.dumps(verification))


if __name__ == '__main__':
    main()
