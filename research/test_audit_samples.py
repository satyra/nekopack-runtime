"""Audit checks only: these tests do not validate the external DAT cipher."""
import io
import struct
import unittest
import zlib
from audit_samples import inspect_archive, mng_info


class PlainDecoder:
    dirs = []

    @staticmethod
    def decrypt(data, key):
        if key:
            raise ValueError('fixture must be plaintext')
        return data


def archive(payload=b'abcd', declared=None):
    length = len(payload) if declared is None else declared
    index = struct.pack('<4I', 1, 1, 2, len(payload))
    return b'NEKOPACK'+struct.pack('<4I', 7, 0, 0, len(index))+index+struct.pack('<2I', 0, length)+payload


def chunk(kind, body):
    return struct.pack('>I', len(body))+kind+body+struct.pack('>I', zlib.crc32(kind+body))


class AuditTests(unittest.TestCase):
    def test_plain_resource_index(self):
        data = archive()
        result = inspect_archive(io.BytesIO(data), 0, len(data), PlainDecoder)
        self.assertEqual(result['entries'], 1)
        self.assertEqual(result['trailing_bytes'], 0)
        self.assertEqual(result['directories'], {'00000001': 1})

    def test_block_length_mismatch(self):
        data = archive(declared=5)
        with self.assertRaisesRegex(ValueError, 'length mismatch'):
            inspect_archive(io.BytesIO(data), 0, len(data), PlainDecoder)

    def test_truncated_payload(self):
        data = archive()[:-1]
        with self.assertRaisesRegex(ValueError, 'outside file'):
            inspect_archive(io.BytesIO(data), 0, len(data), PlainDecoder)

    def test_directory_count_outside_index(self):
        data = bytearray(archive())
        struct.pack_into('<I', data, 28, 2)
        with self.assertRaisesRegex(ValueError, 'count outside index'):
            inspect_archive(io.BytesIO(data), 0, len(data), PlainDecoder)

    def test_mng_crc_and_header(self):
        data = b'\x8aMNG\r\n\x1a\n'+chunk(b'MHDR', struct.pack('>7I', 1, 1, 60, 0, 0, 0, 1))+chunk(b'MEND', b'')
        self.assertEqual(mng_info(data)['mhdr'][:3], [1, 1, 60])
        corrupted = bytearray(data)
        corrupted[20] ^= 1
        with self.assertRaisesRegex(ValueError, 'CRC mismatch'):
            mng_info(corrupted)

    def test_truncated_mng(self):
        with self.assertRaisesRegex(ValueError, 'truncated MNG'):
            mng_info(b'\x8aMNG\r\n\x1a\n'+b'123')


if __name__ == '__main__':
    unittest.main()
