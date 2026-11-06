"""Pin the inspected original shake defaults and opaque snapshot/blit path.
Static evidence only: not original screenshots or frame-scheduling equivalence.
"""
import argparse,hashlib,json
from pathlib import Path
import pefile,capstone

def main():
 p=argparse.ArgumentParser();p.add_argument('--games',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 pinned=json.loads(Path('research/original-shake-math-verification.json').read_text())
 md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32);rows=[]
 for row in pinned['games']:
  paths=list(a.games.rglob(row['file']));assert len(paths)==1
  raw=paths[0].read_bytes();assert hashlib.sha256(raw).hexdigest()==row['sha256']
  pe=pefile.PE(data=raw);base=pe.OPTIONAL_HEADER.ImageBase
  checks=[]
  if row['file']=='Tiny_BoS.exe':
   expected=[(0x49576a,'call','0x4846e4'),(0x49585b,'call','0x59a62c'),
             (0x59a651,'call','0x59a8b8'),(0x59a8e0,'call','0x59a398'),
             (0x59a934,'call','0x59aac8'),(0x59abe5,'cmp','eax, 0xff000000'),
             (0x59abea,'jae','0x59ac17'),(0x59ac17,'mov','dword ptr [edi], eax')]
   spans=[(0x4846e4,0x47),(0x4957b4,0xf8),(0x59a8b8,0xa4),(0x59aac8,0x164)]
  else:
   delta=-0xa4 if row['file']=='Tiny_BaW.exe' else 0
   expected=[(0x441449+delta,'mov','dword ptr [ebp - 0x148], 8'),
             (0x441453+delta,'mov','dword ptr [ebp - 0x14c], 8'),
             (0x44145d+delta,'mov','dword ptr [ebp - 0x150], 0xfa'),
             (0x441824+delta,'mov','dword ptr [ebp - 0x150], 0x32'),
             (0x4417d7+delta,'idiv','ecx'),(0x4417d9+delta,'mov','dword ptr [ebp - 0x148], eax'),
             (0x4417eb+delta,'idiv','ecx'),(0x4417ed+delta,'mov','dword ptr [ebp - 0x14c], eax')]
   spans=[(0x441430+delta,0x3fe)]
  for va,mnemonic,operands in expected:
   ins=next(md.disasm(pe.get_data(va-base,16),va));assert (ins.mnemonic,ins.op_str)==(mnemonic,operands)
   checks.append(dict(va=hex(va),mnemonic=mnemonic,operands=operands))
  rows.append(dict(file=row['file'],sha256=row['sha256'],checks=checks,
                   inspected_spans=[dict(va=hex(va),bytes=n,sha256=hashlib.sha256(pe.get_data(va-base,n)).hexdigest()) for va,n in spans]))
  print('PASS original shake path',row['file'])
 a.output.write_text(json.dumps(dict(scope='Static inspected code anchors; not original playback',games=rows,
    conclusions=['BRAVE snapshots scene layers before the timed loop',
                 'Opaque source pixels overwrite only the clipped destination rectangle',
                 'Translucent source pixels take a separate original alpha path, unsupported by the new snapshot profile',
                 'The inspected loop exit does not issue a zero-offset blit'],
    limits=['No original-engine screenshot comparison','Host cadence and x87/libm bit equality not established','Current text controls are outside the game canvas']),indent=2)+'\n',encoding='utf8')
if __name__=='__main__':main()
