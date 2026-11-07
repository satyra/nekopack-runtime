"""Verify actual environment looping, channel independence and stop from native events."""
import argparse,hashlib,importlib.util,json
from pathlib import Path
p=argparse.ArgumentParser()
for name in ['report','original','output']:p.add_argument('--'+name,type=Path,required=True)
a=p.parse_args();r=json.loads(a.report.read_text(encoding='utf8'));original=json.loads(a.original.read_text(encoding='utf8'))
assert r['execution']=='native ARM64' and len(r['games'])==1
g=r['games'][0];assert g['index']==0 and len(g['pages'])==598 and g['env_loop_required'] and g['closed_vm_live_bytes']==0
assert [(page['sequence'],effect) for page in g['pages'] for effect in page['effects'] if effect.startswith('env')]==[(564,'env ENV_001'),(593,'env_stop')]
h=g['host_events'];starts=[e for e in h if e['event']=='begin' and e['kind']=='env'];stops=[e for e in h if e['event']=='begin' and e['kind']=='env_stop'];assert len(starts)==len(stops)==1
start=starts[0];stop=stops[0];token=start['token'];playing=next(e for e in h if e['event']=='playing' and e['token']==token);ack=next(e for e in h if e['event']=='ack' and e['token']==token)
assert playing['value']>0 and start['time_ms']<=playing['time_ms']<=ack['time_ms'] and ack['value']<playing['value']
assert any(e['event']=='begin' and e['token']>token and e['time_ms']-playing['time_ms']<playing['value'] for e in h[h.index(ack)+1:]),'script waited for audio completion'
assert any(e['event']=='gain' and e['token']==token and e['value']==50 for e in h)
assert any(e['event']=='looping' and e['token']==token and e['value']==1 for e in h)
progress=[e for e in h if e['event']=='progress' and e['token']==token];assert len(progress)>2 and all(0<=e['value']<=playing['value'] for e in progress)
wraps=[e for before,e in zip(progress,progress[1:]) if e['value']<before['value']];looped=[e for e in h if e['event']=='looped' and e['token']==token]
assert wraps and len(wraps)==len(looped)
for wrap,event in zip(wraps,looped):
 assert wrap['value']==event['value'] and wrap['time_ms']<=event['time_ms']
 assert next(e for e in reversed(h[:h.index(event)]) if e['event']=='progress' and e['token']==token)==wrap
assert wraps[0]['time_ms']-playing['time_ms']>=playing['value']-20
assert not any(e['event']=='completed' and e['token']==token for e in h)
source=next(x for x in original['resources'] if x['name']=='env_001');audio=next(x for x in g['audio'] if x['kind']=='env')
assert audio['name']=='ENV_001' and audio['sha256']==source['sha256']
assert abs(playing['value']-source['duration_seconds']*1000)<20
bgm_changes=[e for e in h if e['event']=='playing' and e['kind']=='bgm' and playing['time_ms']<e['time_ms']<stop['time_ms']];assert bgm_changes
assert any(e['time_ms']>bgm_changes[-1]['time_ms'] and e['value']>0 for e in progress)
voices=[e for e in h if e['event']=='playing' and e['kind']=='voice' and playing['time_ms']<e['time_ms']<stop['time_ms']];assert voices
assert any(e['time_ms']>voices[0]['time_ms'] and e['value']>0 for e in progress)
for event in [start,stop]:
 assert any(e['event']=='peer_playing' and e['kind']=='bgm' and e['token']==event['token'] for e in h)
path=Path(__file__).resolve().parents[1]/'platforms/android/verify_story.py';spec=importlib.util.spec_from_file_location('story_verify',path);v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
stop_ack=next(e for e in h if e['event']=='ack' and e['token']==stop['token']);v.verify_channel_stop(h,stop,stop_ack,'env')
assert next(e for e in h if e['event']=='begin' and e['kind']=='bgm_stop' and e['token']>stop['token'])['time_ms']>stop_ack['time_ms']
result=dict(scope='BLESS native ARM64 player position/loop/peer-channel evidence; not an acoustic or seamless-loop parity claim',report_sha256=hashlib.sha256(a.report.read_bytes()).hexdigest(),apk_sha256=r['build']['sha256']['nekopack-arm64-probe.apk'],start_page=564,stop_page=593,environment_token=token,stop_token=stop['token'],duration_ms=playing['value'],command_volume_percent=50,progress_samples=len(progress),observed_wraps=len(wraps),first_wrap_after_start_ms=wraps[0]['time_ms']-playing['time_ms'],bgm_replacements_while_env=len(bgm_changes),voice_starts_while_env=len(voices),ack_before_audio_completion=True,bgm_playing_after_env_stop=True,no_late_env_after_stop=True,vm_live_bytes=g['closed_vm_live_bytes'])
a.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf8');print('PASS native environment evidence',len(wraps),'loops')
