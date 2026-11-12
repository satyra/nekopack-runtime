"""Read-only ED DLL/archive-redirection evidence; exports metadata and system code only.

Requires pefile, capstone and the existing reference portkit decoder. Never reads
scenario body payloads or image payloads, and never runs the original executable.
"""
import argparse
import difflib
import hashlib
import json
from pathlib import Path
import sys

import capstone
import pefile


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--game', type=Path, required=True)
    p.add_argument('--reference-root', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    sys.path.insert(0, str(a.reference_root))
    from portkit.neko import Archive

    dll = (a.game / 'ed_chs.dll').read_bytes()
    pe = pefile.PE(data=dll)
    image = pe.get_memory_mapped_image()
    base = pe.OPTIONAL_HEADER.ImageBase
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
    def instructions(start, end):
        return list(md.disasm(image[start:end], base + start))
    def at(rva, mnemonic, operand):
        item = instructions(rva, rva + 16)[0]
        assert (item.mnemonic, item.op_str) == (mnemonic, operand), (hex(rva), item)
    # Version-specific assertions are intentional: a changed DLL must be reviewed.
    names = {'script.dat': 0x1230, 'script_chs.dat': 0x1248,
             'system.dat': 0x1270, 'system_chs.dat': 0x1288}
    for name, rva in names.items():
        raw = (name + '\0').encode('utf-16le')
        assert image[rva:rva+len(raw)] == raw
    at(0x1532, 'mov', 'edx, 0x10001230')
    at(0x1559, 'push', '0x10001248')
    at(0x155e, 'call', 'dword ptr [0x10003100]')
    at(0x1564, 'mov', 'esp, ebp')
    at(0x1567, 'ret', '0x1c')
    at(0x156c, 'mov', 'edx, 0x10001270')
    at(0x1592, 'push', '0x10001288')
    at(0x1597, 'call', 'dword ptr [0x10003100]')
    at(0x159d, 'mov', 'esp, ebp')
    at(0x15a0, 'ret', '0x1c')
    at(0x15a3, 'push', 'dword ptr [ebp + 8]')
    at(0x15a6, 'call', 'dword ptr [0x10003100]')
    at(0x161d, 'call', '0x10001500')
    imports = {item.name.decode(): item.address for group in pe.DIRECTORY_ENTRY_IMPORT
               for item in group.imports if item.name}
    assert imports['CreateFileA'] == base + 0x403c
    assert imports['CreateFileW'] == base + 0x4038
    at(0x1641, 'mov', 'eax, dword ptr [0x1000403c]')
    at(0x1651, 'mov', 'eax, dword ptr [0x10004038]')
    at(0x1656, 'mov', 'dword ptr [ebp - 0x14], 0x100015c0')
    at(0x1667, 'mov', 'dword ptr [ebp - 8], 0x10001500')
    packs = {name: Archive(a.game / name) for name in names}
    pack_rows = []
    for name, pack in packs.items():
        raw = (a.game / name).read_bytes()
        pack_rows.append(dict(file=name, size=len(raw), sha256=sha(raw),
                              seed=pack.seed, priority=pack.priority, entries=len(pack.entries)))
    system_rows = []
    changed = []
    for name in packs['system_chs.dat'].entries:
        if not name.endswith('.nut'):
            continue
        original = packs['system.dat'].read(name)
        translated = packs['system_chs.dat'].read(name)
        if original != translated:
            changed.append(name)
        lines = translated.splitlines()
        localized = []
        for number, line in enumerate(lines, 1):
            trimmed = line.lstrip()
            gb = trimmed.startswith(b'local t = "Endless Dungeon ') or trimmed.startswith(b'local t = FullChar(info.sdate)') or (trimmed.startswith(b"case '") and b'c="\xa3' in trimmed)
            line.decode('gb18030' if gb else 'cp932')
            if gb:
                localized.append(dict(line=number, sha256=sha(line), encoding='GB18030'))
        system_rows.append(dict(resource=name, original_sha256=sha(original),
                                translated_sha256=sha(translated), gb18030_lines=localized,
                                remaining_lines_encoding='CP932'))
    assert sorted(changed) == ['system/saveload.nut', 'system/system.nut']
    save = packs['system_chs.dat'].read('system/saveload.nut')
    assert b'c="\xa3\xb0"' in save and b'+ "\xa1\xa1" +' in save
    for original, target in [('script.dat', 'script_chs.dat'), ('system.dat', 'system_chs.dat')]:
        # Only resource-name metadata; no scenario/image payload access.
        absent = set(packs[original].entries) - set(packs[target].entries)
        assert not absent, (original, absent)
    boot_rows = []
    for name in ['script/start.txt', 'script/title.txt', 'script/titlelogo.txt']:
        original = packs['script.dat'].read(name)
        translated = packs['script_chs.dat'].read(name)
        assert original == translated
        boot_rows.append(dict(resource=name, original_sha256=sha(original), translated_sha256=sha(translated), byte_identical=True))
    title = packs['script_chs.dat'].read('script/title.txt')
    assert b'graphic\t' + 'クリア'.encode('cp932') in title
    result = dict(scope='Static PE hook, archive metadata, system NUT and boot control code; no scenario body/image payloads',
                  dll_sha256=sha(dll), dll_size=len(dll),
                  redirects=[dict(original='script.dat', target='script_chs.dat'), dict(original='system.dat', target='system_chs.dat')],
                  hook_entrypoints=dict(CreateFileW='0x1500', CreateFileA='0x15c0'),
                  behavior='CreateFileA converts filename to UTF-16 and calls W hook. Each matched replacement call returns directly; only unmatched names pass through. No failed-open fallback or resource-level filtering.',
                  hook_disassembly=[dict(rva=hex(i.address-base), instruction=i.mnemonic+' '+i.op_str) for i in instructions(0x1500, 0x1629)],
                  archives=pack_rows, system_code=system_rows, boot_control_code=boot_rows,
                  map_row_encoding="CP932 resource/condition identifiers; not GB18030 display text",
                  archive_entry_name_sets='Each translated pack covers all original entries; runtime must still prohibit fallback for absent entries.')
    a.output.write_text(json.dumps(result, ensure_ascii=False, indent=2)+'\n', encoding='utf-8')
    print('PASS ED whole-archive hooks, input metadata, and mixed system-code encodings')


if __name__ == '__main__':
    main()
