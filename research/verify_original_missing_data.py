"""Verify original native data/ini empty-resource paths without reading game text/images."""
import argparse
import hashlib
import json
from pathlib import Path

import capstone
import pefile

p = argparse.ArgumentParser()
p.add_argument('--pc', type=Path, required=True)
p.add_argument('--endless', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
profiles = [
    ('Tiny_BaW.exe', 'c570d44929360672475db25ec22704ae06ffc20cbabf4a46d0e60325fbe908d0',
     0x40acb8, 0x40b140, 0x44a394, 0x44a646, 0x44a650, 'je', 0x44aff6,
     0x40ad8c, 0x4cdf94, 0x40b213, 0x4cdf2c),
    ('Tiny_BoD.exe', 'fc2a82cc733ae8fd9585b9e151d7f6a2ca5f1572c2df017e20d330eeff219521',
     0x40ad5c, 0x40b1e4, 0x44a44c, 0x44a6fe, 0x44a708, 'je', 0x44b0ae,
     0x40ae30, 0x4cee5c, 0x40b2b7, 0x4cedf4),
    ('Tiny_BoS.exe', 'a36a6cac08b073c25f2d8e9a7be8cc21b618c43b2485ae18a5af35e36d3cb241',
     0x420428, 0x421758, 0x4c5908, 0x4c6334, 0x4c633e, 'jne', 0x4c642e,
     0x42055b, 0x5ad7a8, 0x421889, 0x5ad6f0),
    ('ed.exe', '921815be7f5f6e6bbedee70daa5b39025acfa78f40897a276beeb6f209e6f12f',
     0x56ec7c, 0x570018, 0x5c0ebc, 0x5c1a72, 0x5c1a7c, 'jne', 0x5c1b6f,
     0x56edb3, 0x44349c, 0x57014d, 0x4433e4),
]
md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
rows = []
for name, digest, data, ini, read, lookup_call, branch, jump, target, array_call, array, table_call, table in profiles:
    path = a.endless if name == 'ed.exe' else next(a.pc.rglob(name))
    raw = path.read_bytes()
    assert hashlib.sha256(raw).hexdigest() == digest, name
    pe = pefile.PE(data=raw)
    base = pe.OPTIONAL_HEADER.ImageBase

    def instructions(va, size):
        off = pe.get_offset_from_rva(va - base)
        return list(md.disasm(raw[off:off + size], va))

    def instruction(va):
        return instructions(va, 16)[0]

    evidence = []
    for caller in (data, ini):
        calls = [i for i in instructions(caller, 370) if i.mnemonic == 'call']
        assert any(i.op_str == hex(read) for i in calls)
        evidence += [dict(address=hex(i.address), instruction=i.mnemonic + ' ' + i.op_str)
                     for i in calls if i.op_str == hex(read)]
    for va, destination in ((array_call, array), (table_call, table)):
        i = instruction(va)
        assert (i.mnemonic, i.op_str) == ('call', hex(destination))
        evidence.append(dict(address=hex(va), instruction=i.mnemonic + ' ' + i.op_str))
    i = instruction(branch)
    assert (i.mnemonic, i.op_str) == (jump, hex(target))
    span = instructions(lookup_call, branch - lookup_call + i.size)
    assert any(i.mnemonic == 'test' and i.op_str == 'al, al' for i in span)
    evidence += [dict(address=hex(i.address), instruction=i.mnemonic + ' ' + i.op_str) for i in span]
    rows.append(dict(executable=name, sha256=digest, data=hex(data), ini=hex(ini),
                     read_resource=hex(read), evidence=evidence,
                     missing_path=('zero return skips resource population and goes to cleanup'
                                   if jump == 'je' else
                                   'zero return falls through cleanup; nonzero jumps to population')))
assert hashlib.sha256(next(a.pc.rglob('Tiny_BfY.exe')).read_bytes()).hexdigest() == profiles[1][1]
result = dict(scope='Four unique original PE binaries; instruction inspection only, no scenario or gallery payloads',
              semantics=dict(Data='[]', Ini='{}'),
              explanation='Resource lookup failure preserves the initialized empty string. Native data/ini parse its zero rows, create an array/table unconditionally, and return one VM value. No filename-specific branch is present.',
              duplicate='Tiny_BfY.exe is byte-identical to Tiny_BoD.exe', binaries=rows)
a.output.write_text(json.dumps(result, indent=2) + '\n', encoding='utf8')
print('PASS original missing Data/Ini semantics:', len(rows), 'unique binaries')
