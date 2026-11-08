"""Pin BLESS BGM descriptor and shared default command-gain branch."""
import argparse
import hashlib
import json
from pathlib import Path
import capstone
import pefile

parser = argparse.ArgumentParser()
parser.add_argument('--games', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
raw = next(args.games.rglob('Tiny_BoD.exe')).read_bytes()
digest = hashlib.sha256(raw).hexdigest()
assert digest == 'fc2a82cc733ae8fd9585b9e151d7f6a2ca5f1572c2df017e20d330eeff219521'
pe = pefile.PE(data=raw)
base = pe.OPTIONAL_HEADER.ImageBase
md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
expected = [(0x4446d4, 'push 0x550a54'), (0x4446d9, 'push 0'),
            (0x4446db, 'push 1'), (0x4446dd, 'push 0x550a50'),
            (0x4446e7, 'call 0x4447dc'),
            (0x44480f, 'mov byte ptr [edx + 8], cl'),
            (0x444818, 'mov byte ptr [eax + 9], dl'),
            (0x445292, 'mov dl, byte ptr [eax + 8]'),
            (0x445297, 'je 0x4452a0'), (0x445299, 'mov ecx, 0x32'),
            (0x4452a0, 'mov ecx, 0x64'), (0x4452a5, 'mov dword ptr [ebp - 0x214], ecx')]
facts = []
for va, wanted in expected:
    instruction = next(md.disasm(pe.get_data(va-base, 16), va))
    actual = (instruction.mnemonic+' '+instruction.op_str).strip()
    assert actual == wanted, (hex(va), actual, wanted)
    facts.append(dict(va=hex(va), instruction=actual))
for va, value in [(0x550a50, b'bgm'), (0x550a54, b'sound/bgm')]:
    assert pe.get_data(va-base, len(value)+1) == value+b'\0'
args.output.write_text(json.dumps(dict(exe_sha256=digest, facts=facts,
    default_command_volume_percent=50,
    limit='Static code evidence only. Config category volume, voice ducking and acoustic parity are not verified here.'), indent=2)+'\n', encoding='utf8')
print('PASS: original BGM default command gain, 12 instruction checks')
