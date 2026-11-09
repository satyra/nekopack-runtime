"""Pin BLESS Config defaults directly from the mounted original DAT priority list."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

p=argparse.ArgumentParser()
p.add_argument('--games',type=Path,required=True)
p.add_argument('--reference-root',type=Path,required=True)
p.add_argument('--output',type=Path,required=True)
a=p.parse_args()
sys.path.insert(0,str(a.reference_root))
from portkit.neko import Archive
sources=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]['sources']
packs=[(s['priority'],Archive(a.games/s['file'])) for s in sources if any(x in s['file'] for x in ['system.dat','patch'])]
choices=[(priority,pack) for priority,pack in packs if 'system/config.nut' in pack.entries]
best=max(p for p,_ in choices);selected=[pack for p,pack in choices if p==best];assert len(selected)==1
raw=selected[0].read('system/config.nut');text=raw.decode('cp932')
checks=['VOLUME_MAX  <- 100','VolBGM    = VOLUME_MAX/2','VolVO     = VOLUME_MAX/2 + VOLUME_MAX/4','VolSE     = VOLUME_MAX/2','VolENV    = VOLUME_MAX/2','VolSYS    = VOLUME_MAX/2']
assert all(c in text for c in checks)
result=dict(scope='BLESS original config.nut from priority-selected raw DAT, static defaults; no acoustic parity claim',source_sha256=hashlib.sha256(raw).hexdigest(),defaults=dict(bgm=50,voice=75,se=50,env=50,system=50),volume_mapping_evidence='research/env-batch23-original.json',limits=['MuteBGM and KeepVoice not implemented in this batch','Original config.sav persistence not implemented','System sound playback is not wired; VolSYS currently updates Config only'])
a.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf8')
print('PASS raw DAT Config defaults',result['source_sha256'])
