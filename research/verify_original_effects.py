"""Verify recorded reverse-engineering facts against user-supplied original PEs.
Requires pefile/capstone. Does not write or redistribute executable bytes.
"""
import argparse,hashlib,json
from pathlib import Path
import pefile,capstone

def main():
 p=argparse.ArgumentParser();p.add_argument('--games',type=Path,required=True);a=p.parse_args()
 evidence=json.loads(Path('research/original-effects-batch10-evidence.json').read_text())
 md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32)
 for row in evidence['games']:
  path=next(a.games.rglob(row['file']));raw=path.read_bytes();assert hashlib.sha256(raw).hexdigest()==row['sha256']
  pe=pefile.PE(data=raw);base=pe.OPTIONAL_HEADER.ImageBase
  for m in row['matches']:
   assert pe.get_data(int(m['fade_string_va'],16)-base,5)==b'fade\0'
   ins=next(md.disasm(pe.get_data(int(m['default_assignment_va'],16)-base,16),int(m['default_assignment_va'],16)))
   assert ins.mnemonic=='mov' and ins.op_str==m['assignment'] and m['default_ms']==4000
  for kernel in row['wipe_kernel_matches'].values():
   assert hashlib.sha256(pe.get_data(int(kernel['va'],16)-base,kernel['bytes'])).hexdigest()==kernel['sha256']
  print('PASS original defaults and identical wipe kernels',row['file'])
if __name__=='__main__':main()
