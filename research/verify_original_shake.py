"""Read-only verification of TD shake math in the four original executables.

Requires pefile/capstone. Reports hashes and addresses, never executable bytes.
Does not assert pixel equivalence or exercise Android.
"""
import argparse
import hashlib
import json
import math
import struct
from pathlib import Path

import capstone
import pefile


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--games', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    pinned = json.loads(Path('research/original-effects-batch10-evidence.json').read_text())
    hashes = {row['file']: row['sha256'] for row in pinned['games']}
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
    addresses = [
        ('Tiny_BoD.exe', 0x441d54, 0x441dae, 0x441e14, 0x441e20, 0x5061f8, 0x504990),
        ('Tiny_BfY.exe', 0x441d54, 0x441dae, 0x441e14, 0x441e20, 0x5061f8, 0x504990),
        ('Tiny_BoS.exe', 0x4957f4, 0x49584e, 0x4958ac, 0x4958b8, 0x5f85ac, 0x5f6cd4),
        ('Tiny_BaW.exe', 0x441caa, 0x441d04, 0x441d6c, 0x441d78, 0x505330, 0x503ac8),
    ]
    rows = []
    parameter_checks = []
    for name, start, end, x_va, y_va, sin_va, trunc_va in addresses:
        paths = list(args.games.rglob(name))
        if len(paths) != 1:
            raise RuntimeError(f'expected one original executable: {name}, found {len(paths)}')
        raw = paths[0].read_bytes()
        digest = hashlib.sha256(raw).hexdigest()
        assert digest == hashes[name], f'unrecognized original executable: {name}'
        pe = pefile.PE(data=raw)
        base = pe.OPTIONAL_HEADER.ImageBase
        def data(va, size):
            return pe.get_data(va-base, size)
        def instructions(va, size):
            return list(md.disasm(data(va, size), va))
        mantissa, exponent = struct.unpack('<QH', data(x_va, 10))
        assert exponent == 0x3ffc
        x = math.ldexp(mantissa / 2**63, exponent-16383)
        y, = struct.unpack('<f', data(y_va, 4))
        assert x == 1/7 and y == 1/8
        kernel = instructions(start, end-start)
        assert kernel[-1].address + kernel[-1].size == end
        assert any(i.mnemonic == 'fld' and i.op_str == f'xword ptr [{hex(x_va)}]' for i in kernel)
        assert any(i.mnemonic == 'fmul' and i.op_str == f'dword ptr [{hex(y_va)}]' for i in kernel)
        calls = [i.op_str for i in kernel if i.mnemonic == 'call']
        assert calls == [hex(sin_va), hex(trunc_va), hex(sin_va), hex(trunc_va)]
        assert any(i.mnemonic == 'fsin' for i in instructions(sin_va, 32))
        rounding = instructions(trunc_va, 48)
        assert any(i.mnemonic == 'or' and i.op_str == 'dword ptr [ebp - 4], 0xc01' for i in rounding)
        assert any(i.mnemonic == 'fistp' for i in rounding)
        if name == 'Tiny_BoS.exe':
            for va, mnemonic, operands in [
                (0x491571, 'mov', 'dword ptr [ebp - 0x1a0], 8'),
                (0x49157b, 'mov', 'dword ptr [ebp - 0x1a4], 8'),
                (0x491585, 'mov', 'dword ptr [ebp - 0x1a8], 0xfa'),
                (0x49206c, 'mov', 'dword ptr [ebp - 0x1a8], 0x32'),
                (0x4957ec, 'cmp', 'ebx, edi'),
                (0x4957ee, 'jge', '0x49587c'),
            ]:
                instruction = instructions(va, 16)[0]
                assert (instruction.mnemonic, instruction.op_str) == (mnemonic, operands)
                parameter_checks.append(dict(file=name, va=hex(va), mnemonic=mnemonic, operands=operands))
        rows.append(dict(file=name, sha256=digest, kernel_va=hex(start), kernel_size=end-start,
                         kernel_sha256=hashlib.sha256(data(start,end-start)).hexdigest(),
                         x_constant_va=hex(x_va), x_mantissa_hex=hex(mantissa),
                         y_constant_va=hex(y_va), x_frequency=x, y_frequency=y,
                         sine_va=hex(sin_va), truncation_va=hex(trunc_va)))
        print('PASS original shake math', name)
    args.output.write_text(json.dumps(dict(
        scope='Static original PE math verification; not original playback or Android validation',
        games=rows,
        brave_defaults_and_exit=parameter_checks,
        limitations=['Snapshot composition and exposed edge pixels need further verification',
                     'Host frame schedule is not verified',
                     'x87 and Rust libm are not asserted bit-identical at every timestamp',
                     'Parameter parser separately inspected in BRAVE; not all four parsers verified']),
        indent=2)+'\n', encoding='utf8')


if __name__ == '__main__':
    main()
