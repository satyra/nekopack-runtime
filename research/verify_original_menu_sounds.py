"""Verify BLESS menu and settings sounds against the original EXE and DAT files."""

import argparse
import hashlib
import json
import subprocess
from pathlib import Path

import capstone
import pefile


EXE_SHA256 = "fc2a82cc733ae8fd9585b9e151d7f6a2ca5f1572c2df017e20d330eeff219521"
SOUND_ASSETS = {
    "PAGE": (35, "317eab82ffc4478ffead11e490aed91e5a208d64f8bd69d97eabc66ddaad7de6"),
    "CLICK": (84, "df096194cb105106495747fa87c19ba5ad8425a1bf38cc8e889a7a16ef0d9ddb"),
    "OK": (87, "df096194cb105106495747fa87c19ba5ad8425a1bf38cc8e889a7a16ef0d9ddb"),
    "SELECT": (57, "0b6855c0f70942523d0f49cba9a6c2992b376faea8d056538515da354154dd7a"),
}
CHECKS = {
    "shortcut.nut": {
        "ViewMenu": {
            91: "function ViewMenu(){",
            92: 'sound("PAGE")',
            109: "local id = spots.Check()",
            113: "if((ev==ID_OK) && id)return InvokeCommand(id)",
            114: 'if(ev==ID_CANCEL){ sound("PAGE"); return false }',
        },
    },
    "config.nut": {
        "shortcut_popup": {
            78: 'm_spots.Bind("DELETE", ID_CANCEL)',
            84: "local id = m_spots.Check()",
            85: "if(event()==ID_OK){",
            86: 'if(id==ID_CANCEL){ sound("CLICK"); return ""; }',
            87: 'if(id){ sound("CLICK"); return m_list[id-1]; }',
        },
        "ViewConfigPage": {
            123: 'spots.Bind("CLOSE",   ID_CANCEL)',
            124: 'spots.Bind("DEFAULT", ID_RESET)',
            128: 'gui.Bind( "PAGE", ScrollBar(images_, "PAGE", 4) )',
            153: 'gui.Bind( "VolSYS", VolumeBar(images, "VolSYS", VOLUME_MAX) )',
            158: 'gui.Bind( "VoiceCheck",  RadioList(images, VOICE_TAGS) )',
            177: "gui.Update()",
            194: "gui.Get(Config)",
            195: "RestoreVolume()",
            207: 'local npage = gui.Get("PAGE")+1;',
            208: "if(npage && (page != npage)){",
            210: "return npage",
            220: "Config.ShortCuts[id-ID_USER-1] = ShortCutsWindow(images).Select(s,snap)",
            223: "case ID_RESET:",
            230: "case ID_CANCEL: id=0",
            235: "if(event()==ID_CANCEL){",
            237: "return 0",
        },
        "ViewConfig": {
            247: "function ViewConfig(page){",
            248: 'sound("PAGE")',
            249: 'while(page){ page = ViewConfigPage(page); sound("PAGE") }',
        },
    },
    "gui.nut": {
        "ScrollBar": {
            27: "class ScrollBar{",
            44: "if(ev==ID_DRAG && id==ID_OK)code=ID_OK",
            46: "case ID_OK:   m_cur=m_spots.GetPosV(m_max+1); break",
            47: "case ID_NEXT: if(m_cur<m_max)++m_cur; break",
            48: "case ID_BACK: if(0<m_cur)--m_cur; break",
        },
        "RadioButton": {
            114: "class RadioButton{",
            123: 'if(m_old != id){ if(m_old = id)sound("SELECT"); }',
            124: 'if(event()==ID_OK && id){ sound("OK"); m_cur = id-1 }',
        },
        "RadioList": {
            229: "class RadioList{",
            233: "m_gui.Bind( name, RadioButton(images, name, 2) )",
        },
        "VolumeBar": {
            253: "class VolumeBar{",
            266: "local code = (ev==ID_OK) ? id : 0",
            267: "if(ev==ID_DRAG && id==ID_OK)code=ID_OK",
            269: 'case ID_OK:   sound("CLICK"); m_cur=m_spots.GetPosH(m_max); break',
            270: 'case ID_NEXT: sound("CLICK"); m_cur=m_max; break',
            271: 'case ID_BACK: sound("CLICK"); m_cur=0; break',
        },
    },
    "system.nut": {
        "InvokeCommand": {
            66: "function InvokeCommand(code){",
            70: "case ID_MENU: ViewMenu(); return true",
            71: "case ID_CONFIG: ViewConfig(1); return true",
            91: 'case ID_AUTO: sound("CLICK"); SwitchAuto(); MegaSkip = false; return true',
            92: 'case ID_SKIP: sound("CLICK"); SwitchSkip(); MegaSkip = false; return true',
        },
    },
}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def archive_entry(cli, archive, folder, name):
    lookup = subprocess.check_output(
        [str(cli), "lookup-name", folder.encode().hex(), name.encode().hex(), str(archive)]
    ).splitlines()
    fields = lookup[0].split(b"\t")
    assert fields[:2] == [b"selected", b"0"], (name, lookup)
    ordinal = int(fields[2])
    return ordinal, subprocess.check_output([str(cli), "read-entry", str(archive), str(ordinal)])


def sound_function(raw):
    pe = pefile.PE(data=raw)
    base = pe.OPTIONAL_HEADER.ImageBase
    disassembler = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
    disassembler.detail = True
    instructions = {
        0x4468CD: ("push", "0x550af0"),
        0x4468D3: ("mov", "ebx, 0x5845e0"),
        0x4468F7: ("push", "0x550ae7"),
        0x4468FD: ("call", "0x4492a4"),
        0x446918: ("push", "0x550af6"),
        0x44691D: ("call", "0x4065d0"),
        0x446924: ("lea", "esi, [ebx + 0x60]"),
        0x446928: ("call", "0x4c0478"),
        0x44694E: ("call", "0x4c0024"),
    }
    restore_instructions = {
        0x406EAF: ("push", "0x40b9d4"),
        0x406EB4: ("push", "0x54835a"),
        0x406EBB: ("call", "0x40a7ac"),
        0x40B9D7: ("call", "0x446a88"),
        0x446AF6: ("push", "0x550b25"),
        0x446AFB: ("call", "0x4065d0"),
        0x446B02: ("push", "0x584640"),
        0x446B07: ("call", "0x4c0478"),
        0x4C0487: ("mov", "eax, dword ptr [ebx + 8]"),
        0x4C048A: ("test", "eax, eax"),
        0x4C0494: ("je", "0x4c04db"),
        0x4C04BB: ("mov", "eax, dword ptr [edx + 8]"),
        0x4C04BE: ("add", "eax, 0x6c"),
        0x4C04C3: ("mov", "dword ptr [eax + 4], edx"),
        0x4C04C9: ("mov", "dword ptr [eax], ecx"),
        0x4C04CE: ("mov", "dword ptr [eax + 8], edx"),
        0x4C04D1: ("add", "eax, 0xc"),
        0x4C04D5: ("call", "0x4b5558"),
        0x4C04DB: ("mov", "dword ptr [ebx + 0xc], edi"),
        0x4B555F: ("call", "0x54724a"),
        0x4B5564: ("mov", "dword ptr [ebx], eax"),
        0x54724A: ("jmp", "dword ptr [0x5af538]"),
    }
    decoded = {}
    for va, expected in (instructions | restore_instructions).items():
        instruction = next(disassembler.disasm(pe.get_data(va - base, 16), va))
        assert (instruction.mnemonic, instruction.op_str) == expected, hex(va)
        decoded[va] = instruction
    system_channel = decoded[0x4468D3].operands[1].imm + decoded[0x446924].operands[1].mem.disp
    assert decoded[0x446B02].operands[0].imm == system_channel
    assert decoded[0x446928].operands[0].imm == decoded[0x446B07].operands[0].imm

    strings = {0x550AF0: "se_%s", 0x550AE7: "sound/se", 0x550AF6: "VolSYS"}
    restore_strings = {0x54835A: "RestoreVolume", 0x550B25: "VolSYS"}
    for va, value in (strings | restore_strings).items():
        assert pe.get_data(va - base, len(value) + 1) == value.encode() + b"\0", hex(va)
    timer_import = next(
        item for directory in pe.DIRECTORY_ENTRY_IMPORT
        for item in directory.imports if item.name == b"timeGetTime"
    )
    assert timer_import.address == decoded[0x54724A].operands[0].mem.disp
    return {
        "instructions": {hex(va): " ".join(instruction) for va, instruction in instructions.items()},
        "strings": {hex(va): value for va, value in strings.items()},
        "restore_volume": {
            "instructions": {
                hex(va): " ".join(instruction) for va, instruction in restore_instructions.items()
            },
            "strings": {hex(va): value for va, value in restore_strings.items()},
            "shared_system_channel": hex(system_channel),
            "timer_import": {hex(timer_import.address): timer_import.name.decode()},
            "active_object_update": (
                "RestoreVolume reads VolSYS and calls the same setter on the system channel as sound. "
                "With an active object the setter writes its gain state and stores a timeGetTime timestamp. "
                "It also stores the channel gain for future playback."
            ),
        },
    }


def script_evidence(cli, archive, scripts):
    evidence = {}
    for name, groups in CHECKS.items():
        ordinal, raw = archive_entry(cli, archive, "system", name)
        export = (scripts / name).read_bytes()
        source = raw.decode("cp932").replace("\r", "")
        assert source == export.decode("utf-8").replace("\r", ""), name
        lines = source.splitlines()
        for group, checks in groups.items():
            for line, expected in checks.items():
                assert lines[line - 1].strip() == expected, (name, group, line)

        # Check for unexpected calls within the sections whose silence is significant.
        if name == "shortcut.nut":
            assert sound_lines(lines, 91, 117) == [92, 114]
        elif name == "config.nut":
            assert sound_lines(lines, 81, 94) == [86, 87]
            assert sound_lines(lines, 111, 241) == []
            assert sound_lines(lines, 247, 254) == [248, 249]
        elif name == "gui.nut":
            assert sound_lines(lines, 27, 67) == []
            assert sound_lines(lines, 114, 134) == [123, 124]
            assert sound_lines(lines, 253, 288) == [269, 270, 271]

        evidence[name] = {
            "ordinal": ordinal,
            "raw_dat_sha256": sha256(raw),
            "export_sha256": sha256(export),
            "normalized_line_locations": {group: list(checks) for group, checks in groups.items()},
        }
    return evidence


def sound_lines(lines, first, last):
    return [line for line in range(first, last + 1) if 'sound("' in lines[line - 1]]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("games", "scripts", "cli", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    exe = next(args.games.rglob("Tiny_BoD.exe"))
    raw = exe.read_bytes()
    assert sha256(raw) == EXE_SHA256
    exe_evidence = sound_function(raw)
    scripts = script_evidence(args.cli, exe.with_name("system.dat"), args.scripts)
    sounds = {}
    for cue, (expected_ordinal, expected_sha256) in SOUND_ASSETS.items():
        name = f"se_{cue}.ogg"
        ordinal, data = archive_entry(args.cli, exe.with_name("sound.dat"), "sound/se", name)
        digest = sha256(data)
        assert data.startswith(b"OggS") and ordinal == expected_ordinal and digest == expected_sha256, cue
        sounds[cue] = {
            "resource": f"sound/se/{name}",
            "ordinal": ordinal,
            "sha256": digest,
            "bytes": len(data),
            "category": "VolSYS",
        }
    report = {
        "scope": "BLESS original EXE and DAT static evidence",
        "exe_sha256": EXE_SHA256,
        "sound_function": exe_evidence,
        "scripts": scripts,
        "sounds": sounds,
        "triggers": {
            "menu": [
                "PAGE on entry and ID_CANCEL.",
                "ID_OK on an enabled hit dispatches its command without a general menu selection cue.",
                "The AUTO and SKIP commands play CLICK in InvokeCommand.",
            ],
            "settings": [
                "PAGE on entry and each return from ViewConfigPage, including exit.",
                "The page scrollbar is silent itself. ViewConfigPage returns on an actual page change; a same-page selection is silent.",
                "The default action changes settings silently.",
                "RadioButton plays SELECT when the hover hit changes to a nonzero option and OK on any option confirmation, even when already selected. RadioList uses RadioButton.",
                "VolumeBar plays CLICK on track confirmation, drag events hitting the track, and endpoint confirmation.",
                "VolumeBar starts CLICK at the prior VolSYS. On the sound page gui.Get(Config) then writes the new value and RestoreVolume updates the active system sound gain.",
            ],
            "shortcut_picker": [
                "Opening a shortcut picker is silent.",
                "Confirming a valid choice or DELETE plays CLICK. Unbound hits are silent.",
            ],
        },
        "limits": [
            "The evidence is static. Windows live timing and listening comparisons were not performed.",
            "The static call order does not establish whether any samples were output at the prior VolSYS before the active gain was updated.",
            "SELECT is verified as an original resource and hover trigger. The Android touch adaptation does not synthesize hover.",
            "This evidence covers ViewMenu and ViewConfig. Original title-map controls and other command screens are outside this check.",
        ],
    }
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print("PASS original BLESS menu/settings scripts, EXE sound binding and four Ogg resources")


if __name__ == "__main__":
    main()
