"""Check actual one-shot completion, SE looping, channel-local stop and following voice."""
import argparse,hashlib,importlib.util,json
from pathlib import Path
p=argparse.ArgumentParser()
for name in ['report','original','output']:p.add_argument('--'+name,type=Path,required=True)
a=p.parse_args();r=json.loads(a.report.read_text(encoding='utf8'));o=json.loads(a.original.read_text(encoding='utf8'));assert r['execution']=='native ARM64' and len(r['games'])==1
g=r['games'][0];assert g['index']==0 and len(g['pages'])==793 and g['se_loop_required'] and g['closed_vm_live_bytes']==0
h=g['host_events'];audio_begins=[e for e in h if e['event']=='begin' and e['kind'] in ['bgm','env','se','voice']];assert len(audio_begins)==len(g['audio']);entries=list(zip(audio_begins,g['audio']))
se=[(e,m) for e,m in entries if e['kind']=='se' and m['name']=='SE_028'];assert len(se)==2
path=Path(__file__).resolve().parents[1]/'platforms/android/verify_story.py';spec=importlib.util.spec_from_file_location('story_verify',path);v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
for index,(begin,meta) in enumerate(se):
 token=begin['token'];assert meta['sha256']==o['resource']['sha256'];playing=next(e for e in h if e['event']=='playing' and e['token']==token);ack=next(e for e in h if e['event']=='ack' and e['token']==token)
 assert abs(playing['value']-o['resource']['duration_seconds']*1000)<20
 assert playing['time_ms']<=ack['time_ms'] and ack['value']<playing['value']
 assert any(e['event']=='looping' and e['token']==token and e['value']==index for e in h)
 assert any(e['event']=='gain' and e['token']==token and e['value']==100 for e in h)
 if index==0:assert v.completed_audio(h,token)
token=se[1][0]['token'];progress=[e for e in h if e['event']=='progress' and e['token']==token];wraps=[b for a,b in zip(progress,progress[1:]) if b['value']<a['value']];looped=[e for e in h if e['event']=='looped' and e['token']==token];assert wraps and len(wraps)==len(looped)
for wrap,event in zip(wraps,looped):
 assert wrap['value']==event['value'] and wrap['time_ms']<=event['time_ms']
 assert next(e for e in reversed(h[:h.index(event)]) if e['event']=='progress' and e['token']==token)==wrap
assert not any(e['event']=='completed' and e['token']==token for e in h)
stops=[e for e in h if e['event']=='begin' and e['kind']=='se_stop'];assert len(stops)==1;stop=stops[0];ack=next(e for e in h if e['event']=='ack' and e['token']==stop['token']);v.verify_channel_stop(h,stop,ack,'se')
assert any(e['event']=='peer_playing' and e['kind']=='bgm' and e['token']==stop['token'] for e in h)
assert not any(e['event']=='begin' and e['kind']=='bgm' and se[1][0]['time_ms']<e['time_ms']<stop['time_ms'] for e in h)
voice=next(e for e,m in entries if e['kind']=='voice' and m['name']=='XXA_012802_0000');assert voice['time_ms']>=ack['time_ms']
assert [(p['sequence'],effect) for p in g['pages'] for effect in p['effects'] if effect in ['se SE_028','se SE_028 loop','se_stop']]==[(783,'se SE_028'),(786,'se SE_028 loop'),(789,'se_stop')]
result=dict(scope='Native player events/position evidence, not acoustic or original dynamic parity',report_sha256=hashlib.sha256(a.report.read_bytes()).hexdigest(),apk_sha256=r['build']['sha256']['nekopack-arm64-probe.apk'],once_page=783,loop_page=786,stop_page=789,following_voice_page=793,once_token=se[0][0]['token'],loop_token=token,stop_token=stop['token'],default_once_completed=True,observed_wraps=len(wraps),gain_percent=100,nonblocking_start_ack=True,bgm_survived_stop=True,no_late_se_after_stop=True,vm_live_bytes=0)
a.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf8');print('PASS SE once/loop/stop and voice',len(wraps),'loops')
