"""Pinned BLESS ResetGame native registration and title request evidence; read-only."""
import argparse,hashlib,json,struct
from pathlib import Path
import capstone,pefile

def main():
 p=argparse.ArgumentParser();p.add_argument('--games',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 files=list(a.games.rglob('Tiny_BoD.exe'));assert len(files)==1
 raw=files[0].read_bytes();digest=hashlib.sha256(raw).hexdigest()
 assert digest=='fc2a82cc733ae8fd9585b9e151d7f6a2ca5f1572c2df017e20d330eeff219521'
 pe=pefile.PE(data=raw);base=pe.OPTIONAL_HEADER.ImageBase;md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32)
 def code(va,size):return list(md.disasm(pe.get_data(va-base,size),va))
 assert pe.get_data(0x54847f-base,10)==b'ResetGame\0'
 registration=code(0x40715e,13)
 assert any(i.mnemonic=='push' and i.op_str=='0x40c518' for i in registration)
 assert any(i.mnemonic=='push' and i.op_str=='0x54847f' for i in registration)
 wrapper=code(0x40c518,12);assert [(i.mnemonic,i.op_str) for i in wrapper if i.mnemonic=='call']==[('call','0x4530a8')]
 body=code(0x4530a8,97);pairs={(i.mnemonic,i.op_str) for i in body}
 assert ('push','0x551e97') in pairs and ('push','0x584710') in pairs
 assert ('call','0x48963c') in pairs and ('mov','byte ptr [0x5510cd], 1') in pairs
 assert pe.get_data(0x551e97-base,6)==b'title\0'
 assert not any(i.mnemonic.startswith('j') for i in body)
 report=dict(exe_sha256=digest,registration_va='0x40715e',wrapper_va='0x40c518',request_va='0x4530a8',target_string='title',request_flag='0x5510cd',body_sha256=hashlib.sha256(pe.get_data(0x4530a8-base,97)).hexdigest(),scope='Pinned native wrapper passes title to 0x48963c and sets a request flag. Does not prove internal VM ownership, audio release, or all native callee behavior.')
 a.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf8');print('PASS original BLESS title-reset registration and request')
if __name__=='__main__':main()
