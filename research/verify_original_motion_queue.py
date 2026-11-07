"""Pin original per-actor FIFO and timer reset instructions; static evidence only."""
import argparse, hashlib, json, sys
from pathlib import Path
import pefile, capstone
p=argparse.ArgumentParser()
for n in ['games','reference-root','output']:p.add_argument('--'+n,type=Path,required=True)
a=p.parse_args();raw=next(a.games.rglob('Tiny_BoD.exe')).read_bytes()
assert hashlib.sha256(raw).hexdigest()=='fc2a82cc733ae8fd9585b9e151d7f6a2ca5f1572c2df017e20d330eeff219521'
pe=pefile.PE(data=raw);base=pe.OPTIONAL_HEADER.ImageBase;md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32)
expected=[(4426146, 'mov dword ptr [ebp - 0x1c4], 0xfa'), (4427544, 'call 0x43a11c'), (4427572, 'mov dword ptr [ebp - 0x198], ecx'), (4429606, 'call 0x43cea4'), (4429645, 'call 0x43ab98'), (4432081, 'inc dword ptr [ebp - 0x1e4]'), (4432101, 'jb 0x438e21'), (4432182, 'call 0x438570'), (4425092, 'push 0x1c'), (4425094, 'push 0'), (4443047, 'call 0x43cddc'), (4443055, 'inc dword ptr [ebx + 0x24]'), (4422688, 'add dword ptr [edi + 0x98], eax'), (4422697, 'add dword ptr [edi + 0x9c], edx'), (4422894, 'rep movsd dword ptr es:[edi], dword ptr [esi]'), (4425017, 'inc dword ptr [ebx + 0x20]'), (4425032, 'dec dword ptr [ebx + 0x24]'), (4425047, 'je 0x437c1d'), (4425057, 'call 0x4b5558'), (4937060, 'mov dword ptr [ebx], eax'), (4425337, 'call 0x437c0c'), (4425354, 'xor edx, edx')]
facts=[]
for va,expected_text in expected:
 i=next(md.disasm(pe.get_data(va-base,16),va));actual=(i.mnemonic+' '+i.op_str).strip();assert actual==expected_text,(hex(va),actual)
 facts.append(dict(va=hex(va),instruction=actual))
sys.path.insert(0,str(a.reference_root));from portkit.neko import Archive
sources=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]['sources']
packs=[(s['priority'],Archive(a.games/s['file'])) for s in sources if any(x in s['file'] for x in ['system.dat','patch'])]
choices=[(p,pack) for p,pack in packs if 'script/012603.txt' in pack.entries];best=max(p for p,_ in choices);selected=[pack for p,pack in choices if p==best];assert len(selected)==1
script=selected[0].read('script/012603.txt');lines=script.decode('cp932').splitlines()
assert lines[210].strip()=='.img2 _ >>>>'
result=dict(scope='BLESS static original FIFO and raw DAT; not original live timing or skip semantics',exe_sha256=hashlib.sha256(raw).hexdigest(),source_sha256=hashlib.sha256(script).hexdigest(),facts=facts,semantics=dict(source='012603',line=211,tokens=['_','>>>>'],segment_duration_ms=250,initial_anchor=[640,360],end_anchor=[840,360],ordering='per-actor FIFO',late_frame='next timed segment starts at current request time; overshoot discarded'),unverified=['original live screen timing','which user actions invoke drain-all'])
a.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf8');print('PASS original queue assertions',len(facts))
