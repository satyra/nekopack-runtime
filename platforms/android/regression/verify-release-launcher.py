"""Install a signed release in place and smoke-test normal library entry points.

Only known UI control bounds are transferred. No screenshots or narrative text.
Make an opaque private-data backup before calling this script.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shlex
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--serial', required=True)
    parser.add_argument('--apk', type=Path, required=True)
    parser.add_argument('--version', required=True)
    parser.add_argument('--game', required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--adb', default='D:/codex/tools/android/sdk/platform-tools/adb.exe')
    args = parser.parse_args()
    prefix = [args.adb, '-s', args.serial]
    package = 'org.nekopack.probe'
    ui = '/sdcard/nekopack-release-check.xml'
    report = dict(version=args.version, passed=False, device=args.serial,
                  apk_sha256=hashlib.sha256(args.apk.read_bytes()).hexdigest(), checks=[],
                  scope='release packaging and normal launcher/title smoke; not full story regression')

    def adb(*parts):
        return subprocess.check_output(prefix + list(parts), stderr=subprocess.STDOUT, timeout=45)

    def bounds(label, attribute='content-desc', timeout=30):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            adb('shell', 'uiautomator', 'dump', ui)
            needle = attribute+'="'+label+'"'
            command = ("sed 's/></>\\n</g' " + shlex.quote(ui) + ' | grep -F ' + shlex.quote(needle)
                       + " | grep -F 'enabled=\"true\"' | sed -n 's/.* bounds=\"\\([^\"]*\\)\".*/\\1/p'")
            rows = adb('shell', 'sh', '-c', shlex.quote(command)).decode().splitlines()
            if rows:
                assert len(rows)==1, ('ambiguous control', label)
                result = tuple(map(int, re.findall(r'\d+', rows[0])))
                assert len(result)==4
                return result
            time.sleep(.3)
        raise AssertionError('control not found: '+label)

    def tap(label, attribute='content-desc'):
        left, top, right, bottom = bounds(label, attribute)
        adb('shell', 'input', 'tap', str((left+right)//2), str((top+bottom)//2))

    def library():
        adb('shell', 'am', 'start', '-W', '-S', '-n', package+'/.LibraryActivity')
        card = bounds('游戏详情 '+args.game)
        # A populated Recently Played section can leave only the bottom of the
        # game tile visible. Reveal the tile before clicking its centre.
        if card[3]-card[1] < 180:
            x = (card[0]+card[2])//2
            adb('shell','input','swipe',str(x),str(card[1]+20),str(x),'180','350')
            card = bounds('游戏详情 '+args.game)
            assert card[3]-card[1] >= 180, 'game tile still clipped'
        return card

    try:
        report['library_before'] = library()
        previous = adb('shell','pm','path',package).decode().strip().removeprefix('package:')
        report['previous_apk_sha256'] = adb('shell','sha256sum',shlex.quote(previous)).split()[0].decode()
        assert b'Success' in adb('install','-r',str(args.apk.resolve()))
        report['checks'].append('in-place install succeeded without uninstall or clearing data')
        installed = adb('shell','pm','path',package).decode().strip().removeprefix('package:')
        assert adb('shell','sha256sum',shlex.quote(installed)).split()[0].decode()==report['apk_sha256']
        denied = subprocess.run(prefix+['shell','run-as',package,'id'],capture_output=True,timeout=30)
        assert denied.returncode!=0 and b'not debuggable' in denied.stdout+denied.stderr
        report['checks'].append('run-as rejected for non-debuggable release')
        denied = subprocess.run(prefix+['shell','am','start','-W','-n',package+'/.MainActivity'],capture_output=True,timeout=30)
        assert b'Permission Denial' in denied.stdout+denied.stderr
        report['checks'].append('external MainActivity launch rejected')
        for label in ('原版','中文补丁'):
            library()
            tap('游戏详情 '+args.game)
            tap(label, 'text')
            tap('启动游戏', 'text')
            bounds('游戏标题菜单', timeout=60)
            report['checks'].append(label+': normal library launch reached original title control')
            adb('shell','input','keyevent','KEYCODE_HOME')
            # Bring the existing task back through its launcher, not a test entry.
            adb('shell','monkey','-p',package,'-c','android.intent.category.LAUNCHER','1')
            # Launcher may show its library or retain the game task; validate the
            # registered game after a controlled cold library start below.
        report['library_after'] = library()
        report['checks'].append('registered game remains in library after both normal launches')
        assert adb('shell','sha256sum',shlex.quote(installed)).split()[0].decode()==report['apk_sha256']
        report['passed'] = True
    finally:
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'passed':True,'version':args.version,'checks':report['checks']},ensure_ascii=False))


if __name__=='__main__':
    main()
