import struct
import unittest
import windows_runtime_dependencies as gate


def fixture(name=b'KERNEL32.dll', delay=False):
    data = bytearray(0x600)
    data[:2] = b'MZ'
    struct.pack_into('<I', data, 60, 0x80)
    data[0x80:0x84] = b'PE\0\0'
    struct.pack_into('<H', data, 0x86, 1)
    struct.pack_into('<H', data, 0x94, 240)
    opt = 0x98
    struct.pack_into('<H', data, opt, 0x20b)
    struct.pack_into('<I', data, opt + 108, 16)
    size = 32 if delay else 20
    struct.pack_into('<II', data, opt + 112 + (13 if delay else 1) * 8, 0x1000, size * 2)
    struct.pack_into('<IIII', data, opt + 240 + 8, 0x400, 0x1000, 0x400, 0x200)
    if delay:
        struct.pack_into('<II', data, 0x200, 1, 0x1080)
    else:
        struct.pack_into('<I', data, 0x20c, 0x1080)
    data[0x280:0x280 + len(name) + 1] = name + b'\0'
    return data


class ImportTests(unittest.TestCase):
    def test_system_and_api_set(self):
        gate.check(fixture(), 'app')
        gate.check(fixture(b'api-ms-win-core-file-l1-2-0.dll', True), 'helper')

    def test_external_normal_and_delay_imports_fail(self):
        for delay in (False, True):
            with self.subTest(delay=delay), self.assertRaisesRegex(ValueError, 'vcruntime140'):
                gate.check(fixture(b'VCRUNTIME140.dll', delay), 'app')

    def test_malformed_rva_and_missing_terminator_fail(self):
        data = fixture()
        struct.pack_into('<I', data, 0x20c, 0xfffffff0)
        with self.assertRaises(ValueError):
            gate.imports(data)
        data = fixture()
        data[0x214:0x228] = b'X' * 20
        with self.assertRaises(ValueError):
            gate.imports(data)

    def test_truncated_section_fails(self):
        with self.assertRaises(ValueError):
            gate.imports(fixture()[:-1])


if __name__ == '__main__':
    unittest.main()
