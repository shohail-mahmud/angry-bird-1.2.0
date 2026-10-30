import struct, sys, os, glob

def conv_chunk(data):
    assert data[:6] == b"\x1bLua\x51\x00"
    endian, int_sz, sizet_sz, instr_sz, num_sz, integral = struct.unpack("6B", data[6:12])
    out = bytearray(b"\x1bLua\x51\x00") + struct.pack("6B", 1, 4, 8, 4, 8, 0)
    pos = 12

    def read_int():
        nonlocal pos
        v = struct.unpack_from("<i", data, pos)[0]
        pos += 4
        return v

    def read_uint32():
        nonlocal pos
        v = struct.unpack_from("<I", data, pos)[0]
        pos += 4
        return v

    def copy_str():
        nonlocal pos
        slen = read_uint32()
        out.extend(struct.pack("<Q", slen))
        if slen > 0:
            out.extend(data[pos:pos+slen])
            pos += slen

    def conv_func():
        nonlocal pos
        copy_str()
        out.extend(data[pos:pos+12]); pos += 12
        ncode = read_int()
        out.extend(struct.pack("<i", ncode))
        out.extend(data[pos:pos + ncode*4]); pos += ncode*4
        nk = read_int()
        out.extend(struct.pack("<i", nk))
        for _ in range(nk):
            t = data[pos]; pos += 1
            out.append(t)
            if t == 0:
                pass
            elif t == 1:
                out.append(data[pos]); pos += 1
            elif t == 3:
                val = struct.unpack_from("<f" if num_sz == 4 else "<d", data, pos)[0]
                pos += num_sz
                out.extend(struct.pack("<d", float(val)))
            elif t == 4:
                copy_str()
            else:
                raise ValueError(f"bad const type {t}")
        np = read_int()
        out.extend(struct.pack("<i", np))
        for _ in range(np):
            conv_func()
        nline = read_int()
        out.extend(struct.pack("<i", nline))
        out.extend(data[pos:pos + nline*4]); pos += nline*4
        nloc = read_int()
        out.extend(struct.pack("<i", nloc))
        for _ in range(nloc):
            copy_str()
            out.extend(data[pos:pos+8]); pos += 8
        nup = read_int()
        out.extend(struct.pack("<i", nup))
        for _ in range(nup):
            copy_str()

    conv_func()
    assert pos == len(data), f"{pos} != {len(data)}"
    return bytes(out)

os.makedirs("/var/tmp/ab10_luac64", exist_ok=True)
for p in sorted(glob.glob("/var/tmp/ab10/Payload/AngryBirds.app/data/*.lua")):
    bn = os.path.basename(p)
    open("/var/tmp/ab10_luac64/" + bn, "wb").write(conv_chunk(open(p, "rb").read()))
print("Converted all 68 files!")
