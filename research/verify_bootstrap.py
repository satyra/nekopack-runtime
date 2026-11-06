"""Pin four startup candidates, verify AST bytes/spans and resolve direct dependencies.
Static dependency evidence only: no script execution or inferred optional imports.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import subprocess


def run(cli, *args, check=True):
    return subprocess.run([str(cli.resolve()), *map(str,args)], capture_output=True, check=check)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--games', required=True, type=Path)
    ap.add_argument('--cli', required=True, type=Path)
    ap.add_argument('--output', required=True, type=Path)
    args = ap.parse_args()
    old = json.loads(Path(__file__).with_name('metadata-verification.json').read_text('utf8'))
    report = {'scope':'bootstrap candidate AST and static direct dependencies; no VM execution',
              'cli_sha256':hashlib.sha256(args.cli.read_bytes()).hexdigest(), 'games':[]}
    for game in old['games']:
        paths = [args.games/s['file'] for s in game['sources']]
        for path, source in zip(paths,game['sources']):
            with path.open('rb') as f:
                assert hashlib.file_digest(f,'sha256').hexdigest() == source['sha256']
        def lookup(directory, name):
            lines = run(args.cli,'lookup-name',directory.hex(),name.hex(),*paths).stdout.decode().splitlines()
            status = lines[0].split('\t')
            result = {'status':status[0], 'candidates':[]}
            for line in lines[1:]:
                _,source,ordinal,priority = line.split('\t')
                result['candidates'].append({'source':int(source),'entry':int(ordinal),'priority':int(priority)})
            if status[0] == 'selected':
                source, entry = map(int,status[1:])
                result.update(source=source, entry=entry, file=game['sources'][source]['file'])
                data = run(args.cli,'read-entry',paths[source],entry).stdout
                result['sha256'] = hashlib.sha256(data).hexdigest()
                return result,data
            return result,None
        startup,data = lookup(b'script',b'start.txt')
        assert data is not None, startup
        parsed = run(args.cli,'parse-bootstrap',paths[startup['source']],startup['entry']).stdout.decode().splitlines()
        expected, statements, offset = [], [], 0
        for number,physical in enumerate(data.splitlines(keepends=True),1):
            body = physical.rstrip(b'\r\n')
            raw = body.strip(b' \t')
            start = offset + len(body)-len(body.lstrip(b' \t'))
            if raw and not raw.startswith(b';'):
                tokens = re.split(rb'[ \t]+',raw)
                expected.append('\t'.join(map(str,[number,start,start+len(raw)]))+'\t'+'\t'.join(t.hex().upper() for t in tokens))
                statements.append((number,start,tokens))
            offset += len(physical)
        assert parsed == expected, 'bootstrap tokens or spans differ'
        row = {'game':game['game'], 'sources':game['sources'], 'startup':startup,
               'statement_count':len(statements), 'commands':dict(Counter(t[0].decode() for _,_,t in statements)),
               'dependencies':[], 'system_command_registrations':[]}
        for line,offset,tokens in statements:
            if tokens[0] not in [b'.call',b'.load',b'.system']:
                continue
            directory = b'system' if tokens[0] == b'.system' else b'script'
            filename = tokens[1] if directory == b'system' else tokens[1]+b'.txt'
            dep,content = lookup(directory,filename)
            dep.update(line=line,offset=offset,command=tokens[0].decode(),target_hex=filename.hex(),directory=directory.decode())
            row['dependencies'].append(dep)
            if content is None:
                continue
            if tokens[0] == b'.call':
                result = run(args.cli,'parse-bootstrap',paths[dep['source']],dep['entry']).stdout.decode().splitlines()
                expected_call, at = [], 0
                for n,physical in enumerate(content.splitlines(keepends=True),1):
                    body = physical.rstrip(b'\r\n')
                    raw = body.strip(b' \t')
                    start = at+len(body)-len(body.lstrip(b' \t'))
                    if raw and not raw.startswith(b';'):
                        fields = re.split(rb'[ \t]+',raw)
                        expected_call.append('\t'.join(map(str,[n,start,start+len(raw)]))+'\t'+'\t'.join(t.hex().upper() for t in fields))
                    at += len(physical)
                assert result == expected_call
                dep['parsed_bootstrap_statements'] = len(result)
            if tokens[0] == b'.system':
                # Anchored, observed direct assignment syntax only; not a Squirrel parser.
                for n,text in enumerate(content.splitlines(),1):
                    match = re.match(rb'\s*com\["([a-zA-Z_0-9]+)"\]\s*(?:=|<-)\s*function\b',text)
                    if match:
                        row['system_command_registrations'].append({'file_hex':filename.hex(),'line':n,'command':match[1].decode()})
            if tokens[0] == b'.load':
                # A bootstrap parser must reject the next-stage script, not silently skip its UI commands.
                attempt = run(args.cli,'parse-bootstrap',paths[dep['source']],dep['entry'],check=False)
                assert attempt.returncode != 0 and attempt.stdout == b''
                dep['bootstrap_subset_rejected_next_stage'] = True
                dep['next_stage_commands'] = dict(Counter(m[1].decode() for m in re.finditer(rb'(?m)^[ \t]*\.([a-zA-Z_0-9]+)',content)))
        report['games'].append(row)
        print(game['game'],len(statements),'statements;',Counter(d['status'] for d in row['dependencies']),flush=True)
    args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf8')


if __name__ == '__main__':
    main()
