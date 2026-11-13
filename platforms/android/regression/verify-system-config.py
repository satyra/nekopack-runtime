"""Exercise mounted original config and nested pages using coordinates/metadata only."""
import argparse,base64,hashlib,importlib.util,json,shlex,struct,uuid,xml.etree.ElementTree as ET
from pathlib import Path
spec=importlib.util.spec_from_file_location('navigation',Path(__file__).with_name('verify-sorakoi-layout-navigation.py'))
nav=importlib.util.module_from_spec(spec);spec.loader.exec_module(nav)
ROOT=Path(__file__).resolve().parents[3]
def decode(data):
    assert data[:4]==b'NKCF';v=data[4]
    if v==13:
        n=struct.unpack_from('<I',data,5)[0];result=decode(data[9:9+n]);tail=data[9+n:];assert len(tail)==12
        actions=['','AUTO','SKIP','LOG','HIDE','SAVE','LOAD','QSAVE','QLOAD','RESET','QUIT','NEXT','BACK','VOICE','CONFIG','DELETE','SCREEN','MENU']
        result['ShortCuts']=[actions[i] for i in tail];return result
    if v in (11,12):
        n=struct.unpack_from('<I',data,5)[0];result=decode(data[9:9+n]);tail=data[9+n:]
        while tail:
            n=tail[0];name=tail[1:1+n].decode();result[name]=struct.unpack_from('<i',tail,1+n)[0];tail=tail[n+5:]
        return result
    if v==10:return decode(data[6:6+data[5]])
    assert v in (6,7,8,9)
    names=['TextSpeed','KeepVoice','VolBGM','VolVO','VolSE','VolENV','VolSYS','BoxOpacity','Bold','Border','AutoSpeed','SkipNoRead']
    return dict(zip(names,data[5:17]))|{'ShortCuts':list(data[17:29])}
def main():
    a=argparse.ArgumentParser(description=__doc__);a.add_argument('--serial',required=True);a.add_argument('--output',type=Path,required=True)
    a.add_argument('--adb',default='D:/codex/tools/android/sdk/platform-tools/adb.exe');args=a.parse_args()
    tree='content://com.android.providers.downloads.documents/tree/raw%3A%2Fstorage%2Femulated%2F0%2FDownload%2FNekoPack-Sorakoi'
    directory='raw:/storage/emulated/0/Download/NekoPack-Sorakoi/NekoPack-Config-Probe';remote='/sdcard/Download/NekoPack-Sorakoi/NekoPack-Config-Probe'
    run='system-config-'+uuid.uuid4().hex[:8];p=nav.NavigationProbe(args.adb,args.serial,run,{})
    report=dict(run=run,passed=False,checks=[])
    extras=['--ei','title_game','0','--es','library_tree',shlex.quote(tree),'--es','library_directory',shlex.quote(directory),'--ez','library_original','true','--ez','dialogue_probe','true','--es','save_probe',run,'--es','read_history_probe',run]
    def events():return p.events('system-command-events.jsonl')
    def named(d,name):return next((b for b in d.get('buttons',[]) if b['name'].split(':')[1]==name),None)
    def state(name,after=0):
        return p.wait('original control '+name,lambda:next((d for d in reversed(events()[after:]) if d.get('has_frame') and named(d,name)),None),timeout=30)
    def point(d,b,fraction=.5):
        layout=p.obj('layout.json');sw,sh=layout['stage_width'],layout['stage_height'];w,h=d['size'];scale=min(sw/w,sh/h)
        x,y,bw,bh=b['bounds'];return round(sw/2+(x+bw*fraction-w/2)*scale),round(sh/2+(y+bh/2-h/2)*scale)
    def tap(d,name,fraction=.5):
        before=len(events());p.tap(*point(d,named(d,name),fraction));return before
    def prefs():
        xml=ET.fromstring(p.private('shared_prefs/MainActivity.xml'))
        values=[e.text for e in xml if e.get('name','').startswith('runtime_config_v1_') and 'NekoPack-Config-Probe' in e.get('name','')]
        assert len(values)==1
        raw=base64.b64decode(values[0]);return raw,decode(raw)
    def start():
        t=p.start_title(*extras);b=next(b for b in t['buttons'] if b['id']==t['start_action']);left,top,right,bottom=p.bounds(nav.startup.TITLE)
        x,y,w,h=b['bounds'];scale=min((right-left)/t['width'],(bottom-top)/t['height']);p.tap(round((left+right)/2+(x+w/2-t['width']/2)*scale),round((top+bottom)/2+(y+h/2-t['height']/2)*scale))
        return state('TextSpeed')
    try:
        manifest=json.loads((ROOT/'.local/system-integration/config-fixture/manifest.json').read_text())
        for item in manifest['files']:
            assert p.adb('shell','sha256sum',shlex.quote(remote+'/'+item['name'])).split()[0].decode()==item['sha256']
        apk=p.adb('shell','pm','path','org.nekopack.probe').decode().strip().removeprefix('package:')
        report['apk_sha256']=p.adb('shell','sha256sum',shlex.quote(apk)).split()[0].decode()
        d=start();raw,before=prefs();report['before']=before
        mark=tap(d,'TextSpeed',.73);d=state('TextSpeed',mark)
        mark=tap(d,'VolBGM',.41);d=state('VOICE',mark)
        mark=tap(d,'VOICE');d=p.wait('original role controls',lambda:next((r for r in reversed(events()[mark:]) if r.get('has_frame') and not named(r,'TextSpeed') and any(b['name'].split(':')[1].startswith('Mute') for b in r.get('buttons',[]))),None))
        mute=next(b['name'].split(':')[1] for b in d['buttons'] if b['name'].split(':')[1].startswith('Mute'));tag=mute[4:]
        mark=tap(d,mute);d=state('Vol'+tag,mark);tap(d,'Vol'+tag,.37)
        mark=len(events());p.adb('shell','input','keyevent','4');d=state('DIALOG',mark)
        report['checks'].append('original role subpage returns without an extra user click')
        mark=tap(d,'DIALOG');d=state('QSAVE',mark);tap(d,'QSAVE')
        mark=len(events());p.adb('shell','input','keyevent','4');d=state('SHORTCUT',mark)
        mark=tap(d,'SHORTCUT');d=state('F1',mark);mark=tap(d,'F1')
        d=state('AUTO',mark);mark=tap(d,'AUTO')
        d=state('F2',mark);mark=tap(d,'F2');d=state('LOG',mark);mark=tap(d,'LOG');d=state('F2',mark)
        mark=len(events());p.adb('shell','input','keyevent','4');d=state('CLOSE',mark)
        tap(d,'CLOSE');p.wait('original config returned',lambda:p.obj('page-1.json'))
        p.wait('save acknowledgement emitted',lambda:any(e.get('save_config') for e in events()))
        raw,after=prefs();report['after']=after;report['role']=tag
        assert 70<=after['TextSpeed']<=76
        assert 37<=after['VolBGM']<=45
        assert after['VoiceMute.'+tag]==1-before['VoiceMute.'+tag]
        assert 32<=after['VoiceVolume.'+tag]<=42
        assert after['Dialog.QSAVE']==1-before['Dialog.QSAVE']
        assert after['ShortCuts'][:2]==['AUTO','LOG']
        report['checks'].append('original root sliders, role mute/volume, dialog and shortcut handlers execute')
        report['saved_sha256']=hashlib.sha256(raw).hexdigest();report['events']=events()
        p=nav.NavigationProbe(args.adb,args.serial,run+'-reload',{});d=start();tap(d,'CLOSE');p.wait('cold config returned',lambda:p.obj('page-1.json'))
        restored_raw,restored=prefs();report['restored']=restored
        assert restored==after
        report['checks'].append('cold restart reexports identical values from original Config after SaveConfig acknowledgement')
        assert not p.read('error.txt');report['reload_run']=p.run
        p=nav.NavigationProbe(args.adb,args.serial,run+'-reset',{});d=start()
        mark=tap(d,'DEFAULT');d=state('OK',mark);mark=tap(d,'OK');d=state('CLOSE',mark)
        tap(d,'CLOSE');p.wait('original default reset returned',lambda:p.obj('page-1.json'))
        _,reset=prefs();report['reset']=reset
        assert reset['TextSpeed']==50 and reset['VolBGM']==75
        assert reset['ShortCuts'][:2]==['CONFIG','VOICE']
        assert all(value==0 for key,value in reset.items() if key.startswith('VoiceMute.'))
        assert all(value==1 for key,value in reset.items() if key.startswith('Dialog.'))
        report['checks'].append('original ResetConfig and nested DEFAULT confirmation restore defaults through SaveConfig')
        assert not p.read('error.txt');report.update(passed=True,reset_run=p.run)
    finally:
        args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    print(json.dumps(dict(passed=True,run=run,checks=report['checks']),ensure_ascii=False))
if __name__=='__main__':main()
