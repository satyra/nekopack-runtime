"""Cross-check BLESS movie completions against the observed route/chapter boundary."""
import argparse,hashlib,json
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);a=p.parse_args()
def load(name):return json.loads((Path('research')/name).read_text(encoding='utf8'))
reference=load('route-batch15-desktop-verification.json');rows=[];builds=set()
for mode,reason in [('natural',0),('skip',2)]:
 name=f'route-batch15-arm64-{mode}-verification.json';report=load(name)
 assert report['execution']=='native ARM64' and len(report['games'])==1
 game=report['games'][0];assert game['index']==0 and len(game['pages'])==12
 assert game['boundary']['checkpoint']==reference['boundary'] and game['boundary']['vm_live_bytes']==0
 assert game['closed_vm_live_bytes']==0 and len(game['movies'])==1
 end=game['movies'][0]['events'][-1]
 assert end['event']=='closed' and end['error'] is None and end['reason']==reason
 if mode=='natural':assert end['frames']==2497 and end['samples']==4590720 and end['head']>=end['samples']
 else:assert 0<end['frames']<2497
 builds.add(report['build']['sha256']['nekopack-arm64-probe.apk'])
 rows.append(dict(report=name,report_sha256_lf=hashlib.sha256((Path('research')/name).read_text(encoding='utf8').encode()).hexdigest(),completion=mode,frames=end['frames'],vm_live_bytes=0,checkpoint=game['boundary']['checkpoint']))
assert len(builds)==1
assert next(iter(builds))==json.loads(Path('.local/android-probe/build.json').read_text())['sha256']['nekopack-arm64-probe.apk']
a.output.write_text(json.dumps(dict(scope='BLESS only, native ARM64; natural and secondary movie completion reach identical route/title and 012401:6 filter boundary; no full save/load or other-game claim',apk_sha256=next(iter(builds)),runs=rows),ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print('PASS BLESS native ARM64 route chapter boundary')
