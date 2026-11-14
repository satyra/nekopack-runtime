"""Cold-load an existing library save and advance without replacing it; metadata only."""
import argparse
import importlib.util
import json
import shlex
import uuid
from pathlib import Path

spec=importlib.util.spec_from_file_location('navigation',Path(__file__).with_name('verify-sorakoi-layout-navigation.py'))
nav=importlib.util.module_from_spec(spec)
spec.loader.exec_module(nav)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--serial',required=True)
    parser.add_argument('--adb',default='D:/codex/tools/android/sdk/platform-tools/adb.exe')
    parser.add_argument('--entry',required=True)
    parser.add_argument('--original',action='store_true')
    parser.add_argument('--slot',type=int,default=101)
    parser.add_argument('--pages',type=int,default=3)
    parser.add_argument('--save-probe',help='existing isolated test save scope')
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    assert args.pages>0
    run='existing-save-'+uuid.uuid4().hex[:8]
    p=nav.NavigationProbe(args.adb,args.serial,run,{})
    result=dict(run=run,passed=False,entry=args.entry,slot=args.slot,original=args.original,pages=[])
    try:
        apk=p.adb('shell','pm','path','org.nekopack.probe').decode().strip().removeprefix('package:')
        result['apk_sha256']=p.adb('shell','sha256sum',shlex.quote(apk)).split()[0].decode()
        extras=['--es','save_probe',shlex.quote(args.save_probe),'--es','read_history_probe',shlex.quote(args.save_probe)] if args.save_probe else []
        p.start_activity('--ei','title_game','0','--es','library_entry',args.entry,
                         '--ez','library_original',str(args.original).lower(),
                         '--ei','library_resume_slot',str(args.slot),'--ez','dialogue_probe','true',*extras)
        for sequence in range(1,args.pages+1):
            page=p.wait('saved route source position',lambda:p.obj(f'page-{sequence}.json'))
            result['pages'].append(page)
            p.wait('text complete',lambda:(t if (t:=p.obj(f'text-{sequence}.json')) and t['complete'] else None))
            if sequence<args.pages:
                layout=p.obj('layout.json')
                p.tap(layout['stage_width']//2,round(layout['stage_height']*.4))
        loaded=p.wait('existing save restored',lambda:(s if (s:=p.obj('save-state.json')) and s['event']=='loaded' else None))
        result.update(passed=True,loaded=loaded)
    finally:
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf8')
    print(json.dumps(result))

if __name__=='__main__':main()
