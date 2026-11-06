"""Read-only original PE evidence for flash colors, count and discrete 30ms phases."""
import argparse,hashlib,json,struct
from pathlib import Path
import pefile,capstone

def main():
 p=argparse.ArgumentParser();p.add_argument('--games',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 pinned=json.loads(Path('research/original-shake-math-verification.json').read_text())
 md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32);rows=[]
 names=['white','白','赤','青','黄','緑','紫','水'];colors=[0xffffff,0xffffff,0xff0000,0xff,0xffff00,0xff00,0xff00ff,0xffff]
 for old in pinned['games']:
  paths=list(a.games.rglob(old['file']));assert len(paths)==1
  raw=paths[0].read_bytes();assert hashlib.sha256(raw).hexdigest()==old['sha256'];pe=pefile.PE(data=raw);base=pe.OPTIONAL_HEADER.ImageBase
  if old['file']=='Tiny_BoS.exe': parser,loop,wait,color_ref=0x4958bc,0x49872a,0x498786,0x4c832a
  elif old['file']=='Tiny_BaW.exe': parser,loop,wait,color_ref=0x441e04,0x4422f4,0x442359,0x44b6c6
  else: parser,loop,wait,color_ref=0x441eac,0x4423ac,0x442408,0x44b77e
  def ins(addr,size):return list(md.disasm(pe.get_data(addr-base,size),addr))
  defaults=ins(parser,80)
  assert any(i.mnemonic=='mov' and i.op_str.endswith(', 1') for i in defaults)
  assert any(i.mnemonic=='mov' and i.op_str.endswith(', 0xffffff') for i in defaults)
  first=ins(wait,8)[0];assert (first.mnemonic,first.op_str)==('push','0x1e')
  # Inspect from the loop's count load to the wait; reject a wrong code anchor.
  body=ins(loop,wait-loop)
  assert any(i.mnemonic=='add' and i.op_str in ['ebx, ebx','esi, esi'] for i in body)
  assert any(i.mnemonic=='test' and i.op_str in ['bl, 1','esi, 1'] for i in body)
  assert any(i.mnemonic=='dec' and i.op_str in ['ebx','esi'] for i in body)
  string_va=struct.unpack('<I',pe.get_data(color_ref-base,4))[0]
  strings=('\0'.join(names)+'\0').encode('cp932');assert pe.get_data(string_va-base,len(strings))==strings
  lookup=ins(color_ref-1,600);assigned=[]
  for i in lookup:
   if i.mnemonic=='mov' and i.op_str.startswith('dword ptr ['):
    value=i.op_str.rsplit(', ',1)[-1]
    if value.startswith('0x') and int(value,16) in colors:assigned.append(int(value,16))
  assert assigned==colors,assigned
  rows.append(dict(file=old['file'],sha256=old['sha256'],parser_va=hex(parser),loop_va=hex(loop),wait_va=hex(wait),phase_wait_ms=30,
                   loop_sha256=hashlib.sha256(pe.get_data(loop-base,wait-loop+10)).hexdigest(),colors=dict(zip(names,colors))))
  print('PASS original flash',old['file'])
 a.output.write_text(json.dumps(dict(scope='Static original executable facts; not original screen captures',games=rows,
   conclusions=['Numeric argument is repeat count, default one','Default RGB is white','Each iteration waits 30ms then alternates solid color and scene snapshot','Count is doubled; last positive-count phase restores snapshot'],
   limits=['Input interruption branches differ among versions','Zero/negative count not supported by the runtime profile','Original DIB alpha is not a display opacity; Android exports opaque RGBA','No guarantee of original frame cadence']),ensure_ascii=False,indent=2)+'\n',encoding='utf8')
if __name__=='__main__':main()
