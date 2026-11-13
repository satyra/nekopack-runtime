"""Read-only PE disassembly for native GUI contract research; no asset reads."""
import argparse
import hashlib
import json
import struct
from pathlib import Path

import capstone
import pefile


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--range", action="append", default=[], help="VA:length, hexadecimal")
    parser.add_argument("--strings", action="store_true")
    parser.add_argument("--verify", action="store_true", help="Assert documented native contract instructions")
    args = parser.parse_args()
    raw = args.exe.read_bytes()
    assert hashlib.sha256(raw).hexdigest() == "9a30908aedf0f10b41ebf1a22c16f48eee840f1d949d2f7b7d8394de20a86d2f"
    pe = pefile.PE(data=raw)
    base = pe.OPTIONAL_HEADER.ImageBase
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
    md.skipdata = True
    if args.verify:
        for name, string_va, name_push, value_push, value in [
            ("ID_OK", 0x75D524, 0x51F080, 0x51F092, 0xFF01),
            ("ID_DRAG", 0x75D52A, 0x51F124, 0x51F136, 0xFF02),
            ("ID_CANCEL", 0x75D532, 0x51F1C8, 0x51F1DA, 0xFF03),
            ("ID_NEXT", 0x75D56E, 0x51F644, 0x51F656, 0xFF0A),
            ("ID_BACK", 0x75D576, 0x51F6EE, 0x51F6FE, 0xFF0B),
            ("ID_NEXT_END", 0x75D57E, 0x51F79C, 0x51F7AC, 0xFF0C),
            ("ID_BACK_END", 0x75D58A, 0x51F84A, 0x51F85A, 0xFF0D),
        ]:
            encoded = name.encode("ascii") + b"\0"
            assert pe.get_data(string_va - base, len(encoded)) == encoded
            for va, operand in [(name_push, string_va), (value_push, value)]:
                instruction = next(md.disasm(pe.get_data(va - base, 15), va))
                assert (instruction.mnemonic, instruction.op_str) == ("push", hex(operand)), hex(va)
        checks = [
            (0x5267C9, "mov", "dword ptr [eax], 0x57f544"),
            (0x52E9CC, "push", "0x58255c"),
            (0x582615, "cmp", "eax, 5"),
            (0x5826C7, "cmp", "eax, 7"),
            (0x5827C8, "call", "0x694208"),
            (0x57F74D, "call", "0x694208"),
            (0x69420F, "push", "0"),
            (0x694211, "push", "0"),
            (0x69422B, "call", "0x6945f8"),
            (0x694621, "call", "0x693fc0"),
            (0x694039, "add", "dword ptr [ebx], eax"),
            (0x694040, "sub", "dword ptr [ecx], edx"),
            (0x694113, "jle", "0x69411b"),
            (0x694BBF, "jle", "0x694d1c"),
            (0x694BC8, "cmp", "edx, 0xff"),
            (0x52B047, "mov", "dword ptr [eax], 0x57fa8c"),
            (0x57FA8F, "call", "0x48412c"),
            (0x48412C, "push", "0x74ba74"),
            (0x484131, "push", "0x74ba84"),
            (0x484136, "call", "0x68d34c"),
            (0x583292, "push", "0x74ba84"),
            (0x583298, "call", "0x68d34c"),
            (0x68D357, "call", "0x68c818"),
            (0x68C8C7, "call", "0x624ff0"),
            (0x68C8E1, "call", "0x6eb280"),
            (0x54A84C, "mov", "dword ptr [eax], 0x58cc10"),
            (0x58CC40, "mov", "eax, 1"),
            (0x58CC50, "mov", "byte ptr [edx + 0x35], al"),
            (0x66E1F9, "mov", "byte ptr [eax + 0x35], 0"),
            (0x673CC5, "jne", "0x673cd2"),
            (0x673CCC, "jne", "0x673d76"),
            (0x673D74, "jmp", "0x673d87"),
            (0x673EA2, "je", "0x673ec7"),
            (0x673EAB, "lea", "edx, [eax + 0xb0]"),
            (0x673EB3, "lea", "edx, [eax + 0x38]"),
            (0x673F2D, "jne", "0x673fa4"),
            (0x67405F, "jne", "0x67408a"),
            (0x694C83, "pmullw", "mm2, mm6"),
            (0x694C89, "psrld", "mm2, 8"),
            (0x694CE5, "mul", "ebx"),
            (0x694CEA, "movd", "mm4, edx"),
            (0x694818, "jae", "0x6947f5"),
            (0x694896, "jae", "0x6948c3"),
            (0x693F7F, "imul", "dword ptr [ebp + 0x10]"),
            (0x694691, "shl", "eax, 2"),
            (0x483EFD, "push", "0x384"),
            (0x483F02, "push", "0x640"),
            (0x483F07, "push", "4"),
            (0x483F0E, "call", "0x68c660"),
            (0x62501D, "call", "0x74184c"),
            (0x51E798, "push", "0x57786c"),
            (0x5778C2, "push", "1"),
            (0x5778C5, "call", "0x483f28"),
            (0x483F5B, "or", "eax, 0xffffffff"),
            (0x483F72, "mov", "dword ptr [0x7a6b94], ecx"),
            (0x483F7A, "je", "0x483fe4"),
            (0x483F7C, "push", "0x4bf"),
            (0x483F88, "call", "0x741978"),
            (0x483F99, "call", "0x74194e"),
            (0x483FDF, "call", "0x74191e"),
            (0x483FF0, "call", "0x741984"),
            (0x484055, "mov", "al, 1"),
            (0x462FF4, "sub", "edx, 0x201"),
            (0x462FFA, "je", "0x463865"),
            (0x463865, "mov", "dword ptr [0x7a6b94], 0xff01"),
        ]
        for va, mnemonic, operands in checks:
            instruction = next(md.disasm(pe.get_data(va - base, 15), va))
            assert (instruction.mnemonic, instruction.op_str) == (mnemonic, operands), hex(va)
        imports = {entry.address: (dll.dll.decode(), entry.name.decode() if entry.name else None)
                   for dll in pe.DIRECTORY_ENTRY_IMPORT for entry in dll.imports}
        for address, name in [(0x7D0560, "MsgWaitForMultipleObjects"),
                              (0x7D0544, "GetMessageA"), (0x7D0524, "DispatchMessageA"),
                              (0x7D0568, "PeekMessageA")]:
            assert imports[address] == ("USER32.DLL", name)
        print(f"PASS native GUI contracts: {len(checks)} instruction assertions, 7 constant registrations; PE SHA-256 pinned")
    if args.strings:
        for key in (b"Draw\0", b"Snap\0", b"SwitchMode\0", b"Sprite\0", b"Screen\0"):
            start = 0
            while (offset := raw.find(key, start)) >= 0:
                va = base + pe.get_rva_from_offset(offset)
                ref = struct.pack("<I", va)
                refs = []
                cursor = 0
                while (at := raw.find(ref, cursor)) >= 0:
                    refs.append(hex(base + pe.get_rva_from_offset(at)))
                    cursor = at + 1
                print(key.decode().rstrip("\0"), hex(va), json.dumps(refs))
                start = offset + 1
    for span in args.range:
        va, length = (int(item, 16) for item in span.split(":"))
        for ins in md.disasm(pe.get_data(va - base, length), va):
            print(f"{ins.address:08x} {ins.mnemonic:8} {ins.op_str}")


if __name__ == "__main__":
    main()
