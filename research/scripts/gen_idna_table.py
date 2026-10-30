"""Generate json_schema_rs/src/validator/format/idna_table.rs (the IDNA2008 code point table)
from the Unicode Character Database files in the directory given as the first argument
(Unicode 17.0, e.g. the copy vendored by sourcemeta-blaze under research/repos/).

Usage: python3 research/scripts/gen_idna_table.py <UCD directory>
Prints the rows of `IDNA_VALID`; paste them between its brackets in idna_table.rs.
"""
import re
import sys

UCD = sys.argv[1]


def parse_prop_file(path, wanted=None):
    out = {}
    for line in open(path, encoding="utf-8"):
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        fields = [f.strip() for f in line.split(";")]
        rng, val = fields[0], fields[1]
        if wanted and val not in wanted:
            continue
        if ".." in rng:
            a, b = rng.split("..")
            a, b = int(a, 16), int(b, 16)
        else:
            a = b = int(rng, 16)
        for cp in range(a, b + 1):
            out[cp] = val
    return out


gc = {}
ccc = {}
first = None
for line in open(f"{UCD}/UnicodeData.txt", encoding="utf-8"):
    f = line.split(";")
    cp = int(f[0], 16)
    name = f[1]
    if name.endswith(", First>"):
        first = cp
        continue
    if name.endswith(", Last>"):
        for c in range(first, cp + 1):
            gc[c] = f[2]
            ccc[c] = int(f[3])
        continue
    gc[cp] = f[2]
    ccc[cp] = int(f[3])

cwkcf = parse_prop_file(f"{UCD}/DerivedNormalizationProps.txt", {"Changes_When_NFKC_Casefolded"})
di = parse_prop_file(f"{UCD}/DerivedCoreProperties.txt", {"Default_Ignorable_Code_Point"})
scripts = parse_prop_file(f"{UCD}/Scripts.txt", {"Greek", "Hebrew", "Hiragana", "Katakana", "Han"})
jt = parse_prop_file(f"{UCD}/extracted/DerivedJoiningType.txt", {"L", "D", "R", "T"})

EXC_PVALID = {0x00DF, 0x03C2, 0x06FD, 0x06FE, 0x0F0B, 0x3007}
EXC_CONTEXTO = {0x00B7, 0x0375, 0x05F3, 0x05F4, 0x30FB} | set(range(0x0660, 0x066A)) | set(range(0x06F0, 0x06FA))
EXC_DISALLOWED = {0x0640, 0x07FA, 0x302E, 0x302F, 0x3031, 0x3032, 0x3033, 0x3034, 0x3035, 0x303B}
OLD_HANGUL = [(0x1100, 0x115F), (0xA960, 0xA97C), (0x1160, 0x11A7), (0xD7B0, 0xD7C6), (0x11A8, 0x11FF), (0xD7CB, 0xD7FB)]
IGNORABLE_BLOCKS = [(0x20D0, 0x20FF), (0x1D100, 0x1D1FF), (0x1D200, 0x1D24F)]
WHITE_SPACE = {0x9, 0xA, 0xB, 0xC, 0xD, 0x20, 0x85, 0xA0, 0x1680, 0x2028, 0x2029, 0x202F, 0x205F, 0x3000} | set(range(0x2000, 0x200B))
LETTER_DIGITS = {"Ll", "Lu", "Lo", "Nd", "Lm", "Mn", "Mc"}


def in_ranges(cp, ranges):
    return any(a <= cp <= b for a, b in ranges)


def derived(cp):
    if cp in EXC_PVALID:
        return "PVALID"
    if cp in EXC_CONTEXTO:
        return "CONTEXTO"
    if cp in EXC_DISALLOWED:
        return "DISALLOWED"
    g = gc.get(cp, "Cn")
    nonchar = (0xFDD0 <= cp <= 0xFDEF) or (cp & 0xFFFE) == 0xFFFE
    if g == "Cn" and not nonchar:
        return "UNASSIGNED"
    if cp == 0x2D or 0x30 <= cp <= 0x39 or 0x61 <= cp <= 0x7A:
        return "PVALID"
    if cp in (0x200C, 0x200D):
        return "CONTEXTJ"
    if cp in cwkcf:
        return "DISALLOWED"
    if cp in di or cp in WHITE_SPACE or nonchar:
        return "DISALLOWED"
    if in_ranges(cp, IGNORABLE_BLOCKS) or in_ranges(cp, OLD_HANGUL):
        return "DISALLOWED"
    if g in LETTER_DIGITS:
        return "PVALID"
    return "DISALLOWED"


JT_BITS = {"L": "JOIN_LEFT", "D": "JOIN_DUAL", "R": "JOIN_RIGHT", "T": "JOIN_TRANSPARENT"}
SCRIPT_BITS = {"Greek": "GREEK", "Hebrew": "HEBREW", "Hiragana": "KANA_HAN", "Katakana": "KANA_HAN", "Han": "KANA_HAN"}


def flags(cp):
    fl = []
    if gc.get(cp, "Cn") in ("Mn", "Mc", "Me"):
        fl.append("MARK")
    if ccc.get(cp, 0) == 9:
        fl.append("VIRAMA")
    if cp in jt:
        fl.append(JT_BITS[jt[cp]])
    if cp in scripts:
        fl.append(SCRIPT_BITS[scripts[cp]])
    return tuple(fl)


rows = []
for cp in range(0x80, 0x110000):
    d = derived(cp)
    if d not in ("PVALID", "CONTEXTO", "CONTEXTJ"):
        continue
    fl = flags(cp)
    if rows and rows[-1][1] == cp - 1 and rows[-1][2] == fl:
        rows[-1][1] = cp
    else:
        rows.append([cp, cp, fl])

print(f"// {len(rows)} ranges", file=sys.stderr)
out = []
for a, b, fl in rows:
    f = " | ".join(fl) if fl else "0"
    out.append(f"    (0x{a:05X}, 0x{b:05X}, {f}),")
print("\n".join(out))
