"""Research-only recipe for the observed Tiny Dungeon system config page.
No UI state conventions are imposed on the generic Rust compositor.
"""
import subprocess


def run(cli, *args):
    return subprocess.run([str(cli.resolve()), *map(str, args)], check=True, capture_output=True).stdout


def unique_sprite(sprites, name):
    found = [i for i, _, _, n in sprites if n == name]
    if len(found) != 1:
        raise ValueError(f'expected one sprite {name!r}; found {len(found)}')
    return found[0]


def sprite_list(cli, archive, entry):
    result = []
    for line in run(cli, 'inspect-sprites', archive, entry).decode('ascii').splitlines():
        ordinal, x, y, name = line.split('\t')
        result.append((int(ordinal), int(x), int(y), bytes.fromhex(name)))
    return result


def config_layers(cli, archive, active_prefix=b'@ '):
    entries, banks = {}, {}
    for filename in ['config.mng', 'config1.mng']:
        lookup = run(cli, 'lookup-name', b'system'.hex(), filename.encode().hex(), archive)
        status, source, ordinal = lookup.decode().splitlines()[0].split('\t')
        if status != 'selected' or source != '0':
            raise ValueError('config resource not uniquely resolved')
        entries[filename] = int(ordinal)
        banks[filename] = sprite_list(cli, archive, ordinal)

    def pick(file, name):
        return f'{entries[file]}:{unique_sprite(banks[file], name)}'

    layers = [pick('config.mng', b'BASE'), pick('config1.mng', b'BASE')]
    layers += [pick('config.mng', n) for n in [b'= PAGE--', b'= PAGE++', b'@ PAGE']]
    values = {b'Screen':0, b'SkipNoRead':0, b'KeepSkip':0, b'KeepAuto':0,
              b'Movie':1, b'Popup':1, b'Effect':1, b'SysEffect':1}
    states = []
    for _, _, _, name in banks['config1.mng']:
        if not name.startswith(b'= '):
            continue
        key, value = name[2:].rsplit(b'#', 1)
        selected = values[key] == int(value)
        chosen = active_prefix + name[2:] if selected else name
        layers.append(pick('config1.mng', chosen))
        states.append({'name_hex': chosen.hex(), 'selected': selected})
    return layers, states
