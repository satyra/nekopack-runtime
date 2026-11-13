"""Read-only native Trans registration/mask proof; never reads game assets."""
import argparse
import hashlib
import json
from pathlib import Path
import capstone
import pefile


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    raw = args.exe.read_bytes()
    assert hashlib.sha256(raw).hexdigest() == "9a30908aedf0f10b41ebf1a22c16f48eee840f1d949d2f7b7d8394de20a86d2f"
    pe = pefile.PE(data=raw)
    base = pe.OPTIONAL_HEADER.ImageBase
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
    checks = []

    def ins(va, mnemonic, operands):
        item = next(md.disasm(pe.get_data(va-base, 15), va))
        assert (item.mnemonic, item.op_str) == (mnemonic, operands), hex(va)
        checks.append(dict(va=hex(va), mnemonic=mnemonic, operands=operands))

    for va, value in [(0x75D385,b".nss\0"),(0x75D379,b".i\0"),
                      (0x7424D8,b"image/mask\0"),(0x75E07A,b"fade\0"),
                      (0x749652,b"png\0"),(0x749656,b"jpg\0")]:
        assert pe.get_data(va-base,len(value)) == value
    for va,mn,op in [
        (0x51EAE5,"push","-2"),(0x51EAEC,"push","0x57c874"),
        (0x51EACA,"push","-1"),(0x51EAD1,"push","0x57c7c8"),
        (0x57C8A0,"mov","esi, 1"),(0x57C904,"je","0x57c953"),
        (0x57C93B,"call","0x6eb838"),(0x57C947,"mov","esi, 2"),
        (0x57C94E,"mov","esi, 3"),(0x57C95B,"call","0x4055e0"),
        (0x4055FE,"je","0x40560a"),(0x405602,"call","0x405410"),
        (0x40560F,"call","0x5757a4"),(0x40561B,"mov","dword ptr [ebx + 0x14], eax"),
        (0x405383,"push","4"),(0x405385,"push","0x7427c4"),
        (0x40538A,"call","0x68c660"),(0x68C6FA,"call","0x624ff0"),
        (0x625013,"push","4"),(0x625015,"push","0x1000"),
        (0x62501B,"push","0"),(0x62501D,"call","0x74184c"),
        (0x40542A,"call","0x68c580"),(0x405447,"call","0x469a08"),
        (0x405453,"push","0"),(0x405455,"push","0"),
        (0x405457,"push","0x7427c4"),(0x40545C,"call","0x69426c"),
        (0x4262FD,"call","0x42a43c"),(0x42A48D,"call","0x405410"),
        (0x46AF14,"call","0x4568dc"),(0x46AF1E,"jne","0x46b029"),
        (0x46AF24,"xor","eax, eax"),(0x46B031,"call","0x68d364"),
        (0x694294,"call","0x69454c"),(0x694575,"call","0x693fc0"),
        (0x693FF2,"cmp","eax, 4"),(0x693FF7,"xor","eax, eax"),
        (0x6945D0,"call","0x694774"),(0x694896,"jae","0x6948c3"),
        (0x69489B,"shr","eax, 0x18"),(0x6948B0,"psubw","mm2, mm1"),
        (0x6948B3,"pmullw","mm2, mm4"),(0x6948B6,"psrlw","mm2, 8"),
        (0x405706,"call","0x401384"),(0x40571C,"call","0x4013fc"),
        (0x4013C9,"paddusb","mm0, mm6"),(0x4013CF,"pcmpeqb","mm0, mm7"),
        (0x401439,"shl","ecx, 1"),(0x40147A,"psubusw","mm0, mm7"),
        (0x401486,"pcmpgtw","mm0, mm7"),(0x4014B5,"psubsw","mm1, mm0"),
        (0x4014BB,"pmullw","mm1, mm4"),(0x4014C1,"psrlw","mm1, 8"),
        (0x4014C9,"paddb","mm0, mm1"),
    ]:
        ins(va,mn,op)
    ins(0x74184C,"jmp","dword ptr [0x7d0390]")
    imports={item.address:(dll.dll.decode(),item.name.decode() if item.name else None)
             for dll in pe.DIRECTORY_ENTRY_IMPORT for item in dll.imports}
    assert imports[0x7D0390] == ("KERNEL32.DLL","VirtualAlloc")
    kernels=[]
    for kind,va,size,expected in [
        ("hard",0x401384,120,"3d57419ceab91ae1e0a7ee47afdeed9f20c86ea49c449ec72bbaef41006ac86f"),
        ("fade",0x4013FC,232,"f2c52fa9cab387e7523972e249ae96e5597662502dbca54c6349ea183c5eea77"),
    ]:
        digest=hashlib.sha256(pe.get_data(va-base,size)).hexdigest()
        assert digest == expected
        kernels.append(dict(kind=kind,va=hex(va),bytes=size,sha256=digest))
    args.output.write_text(json.dumps(dict(scope="static PE proof; no execution or game asset access",
        sha256=hashlib.sha256(raw).hexdigest(),checks=checks,kernels=kernels),indent=2)+"\n",encoding="utf-8")
    print(f"PASS native Trans registration/mask ({len(checks)} instructions, 2 kernel hashes)")


if __name__ == "__main__":
    main()
