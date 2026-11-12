"""Verify the original BRAVE missing-mask path using only PE/disassembly evidence.

Requires pefile/capstone. Never reads scenario text or image resources, and never
writes executable bytes. The pinned PE is the same build as batch10 evidence.
"""
import argparse
import hashlib
import json
from pathlib import Path

import capstone
import pefile


SHA256 = "a36a6cac08b073c25f2d8e9a7be8cc21b618c43b2485ae18a5af35e36d3cb241"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    raw = args.exe.read_bytes()
    assert hashlib.sha256(raw).hexdigest() == SHA256, "unexpected original PE"
    pe = pefile.PE(data=raw)
    base = pe.OPTIONAL_HEADER.ImageBase
    assert pe.FILE_HEADER.Machine == 0x14C and base == 0x400000
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
    checks = []

    def instruction(address, mnemonic, operands):
        ins = next(md.disasm(pe.get_data(address - base, 15), address))
        assert (ins.mnemonic, ins.op_str) == (mnemonic, operands), hex(address)
        checks.append(dict(va=hex(address), mnemonic=mnemonic, operands=operands))

    # Fixed mask surface: initialize once with four channels, 1280 x 720.
    instruction(0x491136, "mov", "edx, dword ptr [0x64885c]")
    instruction(0x49113C, "test", "edx, edx")
    instruction(0x49113E, "jne", "0x491159")
    for address, operand in [(0x491140, "0x2d0"), (0x491145, "0x500"),
                             (0x49114A, "4"), (0x49114C, "0x648854")]:
        instruction(address, "push", operand)
    instruction(0x491151, "call", "0x597f74")
    instruction(0x597FE3, "imul", "esi, ebx")
    instruction(0x598004, "imul", "ecx, dword ptr [ebx + 0x20]")
    instruction(0x59800C, "call", "0x583118")
    for address, operand in [(0x58313B, "4"), (0x58313D, "0x1000"),
                             (0x583142, "edi"), (0x583143, "0")]:
        instruction(address, "push", operand)
    instruction(0x583145, "call", "0x639a10")
    instruction(0x639A10, "jmp", "dword ptr [0x6b838c]")
    imported = {i.address: (dll.dll.decode(), i.name.decode() if i.name else None)
                for dll in pe.DIRECTORY_ENTRY_IMPORT for i in dll.imports}
    assert imported[0x6B838C] == ("KERNEL32.DLL", "VirtualAlloc")

    # Each load begins with an empty temporary image. Load failure returns zero
    # without entering the successful image-materialization branch.
    instruction(0x4911D6, "call", "0x597e94")
    instruction(0x597EBC, "xor", "ecx, ecx")
    instruction(0x597EC3, "mov", "dword ptr [eax + 4], ecx")
    assert pe.get_data(0x648568 - base, 11) == b"image/mask\0"
    instruction(0x4911E9, "push", "0x648568")
    instruction(0x4911F2, "call", "0x4c00f4")
    instruction(0x4C12E9, "call", "0x4b1084")
    instruction(0x4C12F1, "test", "al, al")
    instruction(0x4C12F3, "jne", "0x4c13e3")
    instruction(0x4C12F9, "xor", "eax, eax")
    instruction(0x4C12FB, "push", "eax")
    instruction(0x4C13D0, "pop", "eax")
    instruction(0x4C13DE, "jmp", "0x4c14ee")
    instruction(0x4C13EA, "call", "0x598c50")
    instruction(0x4C14F4, "ret", "")

    # The caller ignores the loader's result and blits the temporary image.
    # Empty source has zero channels; the four-channel blit exits without writes.
    instruction(0x4911F7, "add", "esp, 0xc")
    instruction(0x4911FA, "lea", "eax, [ebp - 0xc]")
    instruction(0x4911FD, "push", "eax")
    instruction(0x491202, "push", "0x648854")
    instruction(0x491207, "call", "0x59a62c")
    instruction(0x59A651, "call", "0x59a8b8")
    instruction(0x59A8E0, "call", "0x59a398")
    instruction(0x59A8E8, "test", "eax, eax")
    instruction(0x59A8EA, "je", "0x59a954")
    instruction(0x59A3C2, "call", "0x59831c")
    instruction(0x59A3C8, "cmp", "eax, 4")
    instruction(0x59A3CB, "je", "0x59a3d4")
    instruction(0x59A3CD, "xor", "eax, eax")
    instruction(0x59A3CF, "jmp", "0x59a4f6")
    instruction(0x598324, "mov", "eax, dword ptr [ebx + 8]")
    instruction(0x598331, "je", "0x598357")
    instruction(0x598357, "xor", "eax, eax")

    output = dict(
        scope="read-only PE headers/imports/disassembly; no scenario/image access",
        executable=args.exe.name, sha256=SHA256,
        mask_surface=dict(address="0x648854", channels=4, width=1280, height=720,
                          allocation="VirtualAlloc(NULL, size, MEM_COMMIT, PAGE_READWRITE)"),
        conclusion="Failed mask loads leave the existing surface unchanged; before the first successful load it is the zero-initialized committed surface.",
        evidence_kind="static control-flow and allocation evidence, not a live rendering observation",
        instruction_checks=checks,
    )
    args.output.write_text(json.dumps(output, indent=2) + "\n", encoding="utf-8")
    print(f"PASS original missing-mask initialization/load/blit path ({len(checks)} instructions)")


if __name__ == "__main__":
    main()
