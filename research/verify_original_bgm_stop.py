"""Verify original empty-name sound branch and immediate handle release, read-only."""
import argparse,hashlib,json
from pathlib import Path
import pefile,capstone

def main():
 p=argparse.ArgumentParser();p.add_argument('--games',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 pinned=json.loads(Path('research/original-effects-batch10-evidence.json').read_text());md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32);rows=[]
 for old in pinned['games']:
  paths=list(a.games.rglob(old['file']));assert len(paths)==1
  raw=paths[0].read_bytes();assert hashlib.sha256(raw).hexdigest()==old['sha256'];pe=pefile.PE(data=raw);base=pe.OPTIONAL_HEADER.ImageBase
  if old['file']=='Tiny_BoS.exe':
   parser,branch,empty,stop,release,clear=0x4a5d24,0x4af181,0x4afab0,0x4afb4a,0x59d3e4,0x59d437
   flags=['byte ptr [ebp - 0x266], 0','byte ptr [ebp - 0x267], 0'];guards=[0x4afb2c,0x4afb3a]
  else:
   shift=0xb4 if old['file']=='Tiny_BaW.exe' else 0
   parser,branch,empty,stop=[x-shift for x in [0x44487c,0x445fe2,0x446598,0x446610]]
   release,clear=(0x4bf4e4,0x4bf53d) if shift else (0x4c03ac,0x4c0405)
   flags=['byte ptr [ebp - 0x20e], 0','byte ptr [ebp - 0x20f], 0'];guards=[0x4465f8-shift,0x446606-shift]
  def instructions(va,size):return list(md.disasm(pe.get_data(va-base,size),va))
  def first(va):return instructions(va,16)[0]
  defaults=instructions(parser,128)
  assert all(any(i.mnemonic=='mov' and i.op_str==flag for i in defaults) for flag in flags)
  assert (first(branch).mnemonic,first(branch).op_str)==('je',hex(empty))
  assert all(first(g).mnemonic=='jne' for g in guards)
  assert (first(stop).mnemonic,first(stop).op_str)==('call',hex(release))
  body=instructions(release,clear-release+3)
  assert any(i.mnemonic=='dec' and i.op_str=='dword ptr [esi + 4]' for i in body)
  assert any(i.mnemonic=='call' and i.op_str=='dword ptr [eax]' for i in body)
  assert any(i.mnemonic=='xor' and i.op_str=='edx, edx' for i in body)
  assert (first(clear).mnemonic,first(clear).op_str)==('mov','dword ptr [ebx + 4], edx')
  rows.append(dict(file=old['file'],sha256=old['sha256'],parser_va=hex(parser),empty_name_branch_va=hex(branch),empty_name_target_va=hex(empty),modifier_guard_vas=list(map(hex,guards)),release_call_va=hex(stop),release_va=hex(release),handle_clear_va=hex(clear),empty_branch_sha256=hashlib.sha256(pe.get_data(empty-base,stop-empty+5)).hexdigest(),release_sha256=hashlib.sha256(pe.get_data(release-base,clear-release+3)).hexdigest()))
  print('PASS original bare BGM stop',old['file'],flush=True)
 a.output.write_text(json.dumps(dict(scope='Original PE static control-flow evidence; not live acoustic capture',games=rows,conclusion='Empty resource name with no volume/pan modifiers releases the selected channel handle immediately; no fade-duration scheduling',limits=['Original shared ownership can defer final object destruction until the last reference','Other sound options and channels are not implemented by this change','Android cancellation of its own fade animator is an adapter requirement, not a claim about original timer internals']),indent=2)+'\n')
if __name__=='__main__':main()
