"""Verify movie-command instruction facts in user-owned originals; never publish binary bytes."""
import argparse,hashlib,json
from pathlib import Path
import pefile,capstone

def main():
 p=argparse.ArgumentParser();p.add_argument('--games',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 profiles=[('Tiny_BoD.exe',0x461504,0x461d68,0x551fcd,0x551fd2,0x4616e9,0x461d81,0x446870,0x44688a,0x4623bd,0x4623c9,0x4623d8,0x4623e5,0x46240e,0x448edc,0x448f29,0x448f39),('Tiny_BfY.exe',0x461504,0x461d68,0x551fcd,0x551fd2,0x4616e9,0x461d81,0x446870,0x44688a,0x4623bd,0x4623c9,0x4623d8,0x4623e5,0x46240e,0x448edc,0x448f29,0x448f39),('Tiny_BoS.exe',0x4ef174,0x4f2090,0x64c624,0x64c629,0x4f0194,0x4f20a6,0x4aff10,0x4aff2a,0x4f35cd,0x4f35d6,0x4f35e2,0x4f35ee,0x4f3617,None,0x4bdf47,0x4bdf62),('Tiny_BaW.exe',0x4607a4,0x461008,0x550f97,0x550f9c,0x460989,0x461021,0x4467bc,0x4467d6,0x46165d,0x461669,0x461678,0x461685,0x4616ae,0x448e28,0x448e75,0x448e85)]
 reports=[]
 for name,parser,execute,skip,empty,sete,stopcall,stopfn,channels,guard,left,right,status,cleanup,mapping,l_event,r_event in profiles:
  path=next(a.games.rglob(name));raw=path.read_bytes();pe=pefile.PE(data=raw);base=pe.OPTIONAL_HEADER.ImageBase;md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32)
  def one(address):return next(md.disasm(pe.get_data(address-base,16),address))
  def require(address,mnemonic,suffix):
   ins=one(address);assert ins.mnemonic==mnemonic and ins.op_str.endswith(suffix),(name,hex(address),ins.mnemonic,ins.op_str)
   return dict(address=hex(address),instruction=f'{ins.mnemonic} {ins.op_str}')
  assert pe.get_data(skip-base,5)==b'skip\0' and pe.get_data(empty-base,1)==b'\0'
  facts=[require(sete,'sete','dl'),require(stopcall,'call',hex(stopfn)),require(channels,'cmp','4'),require(guard,'test','bl, bl'),require(left,'cmp','0xff01'),require(right,'cmp','0xff03'),require(status,'call',one(status).op_str),require(cleanup,'call',one(cleanup).op_str),require(l_event,'mov','0xff01'),require(r_event,'mov','0xff03')]
  branch=one(guard+2);assert branch.mnemonic=='je' and guard<int(branch.op_str,16)<=right
  if mapping:facts.append(require(mapping,'sub','0x201'));assert one(mapping+6).op_str==hex(l_event)
  else:
   found=[]
   for section in pe.sections:
    if not section.Characteristics&0x20000000:continue
    md.skipdata=True
    instructions=list(md.disasm(section.get_data(),base+section.VirtualAddress))
    for j,ins in enumerate(instructions[:-1]):
     if ins.mnemonic=='sub' and ins.op_str.endswith('0x201') and instructions[j+1].op_str==hex(l_event):found.append(ins)
   assert len(found)==1;mapping=found[0].address;facts.append(require(mapping,'sub','0x201'))
  # Following subtraction by 3 maps WM_RBUTTONDOWN (0x204).
  next_sub=one(mapping+6+one(mapping+6).size)
  assert next_sub.mnemonic=='sub' and next_sub.op_str.endswith(', 3')
  assert one(next_sub.address+next_sub.size).op_str==hex(r_event)
  movie=path.parent/'op.mpg'
  reports.append(dict(game=name,exe_sha256=hashlib.sha256(raw).hexdigest(),parser=hex(parser),execute=hex(execute),skip_string=hex(skip),absent_argument_empty=hex(empty),facts=facts,movie_bytes=movie.stat().st_size,movie_sha256=hashlib.sha256(movie.read_bytes()).hexdigest()))
 a.output.write_text(json.dumps(dict(scope='static disassembly of four local originals; not a captured original-player timing test',semantics=['stop all four sound channels on entry','primary mouse exits only with second argument skip','secondary mouse exits regardless of skip flag','player completion exits wait loop, cleanup precedes script return','Movie configuration gates playback; runtime currently assumes enabled'],games=reports),indent=2)+'\n')
if __name__=='__main__':main()
