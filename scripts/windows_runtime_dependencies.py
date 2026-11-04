"""Check imports in the actual packaged PE32+ executables, without loading them."""
import re
import struct
import zipfile

# Windows system DLLs used by this application and its native audio helper.
# A new external dependency must be explicitly reviewed, not inferred from CI.
SYSTEM_DLLS = frozenset('''advapi32 bcrypt bcryptprimitives cfgmgr32 comctl32 comdlg32
combase crypt32 dbghelp dcomp dinput8 dnsapi dwmapi dxgi gdi32 hid imm32 iphlpapi
kernel32 kernelbase mmdevapi ncrypt ntdll ole32 oleaut32 opengl32
powrprof propsys psapi rpcrt4 secur32 setupapi shcore shell32 shlwapi user32
uiautomationcore userenv uxtheme version winhttp winmm winspool wintrust ws2_32 wtsapi32
avrt ksuser mfplat mfreadwrite mfuuid runtimeobject windowscodecs'''.split())
API_SET = re.compile(r'(?:api|ext)-ms-win-[a-z0-9-]+\.dll\Z')


def imports(data):
    def read(fmt, offset):
        size = struct.calcsize(fmt)
        if offset < 0 or offset + size > len(data):
            raise ValueError('Truncated PE')
        return struct.unpack_from(fmt, data, offset)

    if len(data) < 64 or data[:2] != b'MZ':
        raise ValueError('Invalid DOS header')
    pe, = read('<I', 60)
    if data[pe:pe + 4] != b'PE\0\0':
        raise ValueError('Invalid PE signature')
    count, = read('<H', pe + 6)
    opt_size, = read('<H', pe + 20)
    opt = pe + 24
    if not count or opt_size < 112 or opt + opt_size > len(data) or read('<H', opt)[0] != 0x20b:
        raise ValueError('Invalid PE32+ header')
    directories, = read('<I', opt + 108)
    if directories > (opt_size - 112) // 8:
        raise ValueError('Invalid PE data directories')
    sections = opt + opt_size
    if sections + count * 40 > len(data):
        raise ValueError('Truncated PE sections')
    mappings = []
    for i in range(count):
        off = sections + 40 * i
        virtual_size, va, raw_size, raw = read('<IIII', off + 8)
        if raw + raw_size > len(data) or va + max(virtual_size, raw_size) > 0x100000000:
            raise ValueError('Invalid section bounds')
        mappings.append((va, raw_size, raw))

    def rva(address, size):
        matches = [raw + address - va for va, extent, raw in mappings
                   if address >= va and address - va <= extent and size <= extent - (address - va)]
        if len(matches) != 1:
            raise ValueError('Unmapped or ambiguous PE RVA')
        return matches[0]

    def name(address):
        # One byte at a time keeps the terminating NUL inside the mapped section.
        value = bytearray()
        for i in range(256):
            byte = data[rva(address + i, 1)]
            if byte == 0:
                break
            value.append(byte)
        else:
            raise ValueError('Unterminated import name')
        try:
            result = value.decode('ascii').lower()
        except UnicodeDecodeError as exc:
            raise ValueError('Invalid import name') from exc
        if not re.fullmatch(r'[a-z0-9_-]+\.dll', result):
            raise ValueError('Invalid import DLL name')
        return result

    result = set()
    for index, entry_size in ((1, 20), (13, 32)):
        if directories <= index:
            continue
        address, size = read('<II', opt + 112 + index * 8)
        if not address and not size:
            continue
        if not address or size < entry_size or size > 1024 * 1024:
            raise ValueError('Invalid import directory')
        start = rva(address, size)
        for offset in range(start, start + size - entry_size + 1, entry_size):
            entry = data[offset:offset + entry_size]
            if not any(entry):
                break
            if index == 1:
                dll_rva, = struct.unpack_from('<I', entry, 12)
            else:
                attributes, dll_rva = struct.unpack_from('<II', entry)
                if attributes != 1:
                    raise ValueError('Unsupported delay import addressing')
            result.add(name(dll_rva))
        else:
            raise ValueError('Unterminated import directory')
    return result


def check(data, label):
    names = imports(data)
    unknown = sorted(n for n in names if n.removesuffix('.dll') not in SYSTEM_DLLS
                     and (not API_SET.fullmatch(n) or n.startswith('api-ms-win-crt-')))
    if unknown:
        raise ValueError(f'{label}: undeclared DLL imports: {", ".join(unknown)}')
    print(f'{label}: checked imports: {", ".join(sorted(names))}')


def check_archive(path, base):
    with zipfile.ZipFile(path) as archive:
        for name in ('ak-configurator.exe', 'audio/cava.exe'):
            full = base + '/' + name
            if sum(info.filename == full for info in archive.infolist()) != 1:
                raise ValueError('Missing or duplicate packaged executable: ' + full)
            check(archive.read(full), name)
