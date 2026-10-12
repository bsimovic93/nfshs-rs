"""hudprims.py RAM.bin: decode the HUD's pre-built PS1 primitive buffer (*(gp+6356)) from a RAM dump."""
import struct,sys
r=open(sys.argv[1],'rb').read()
R=lambda a:struct.unpack_from('<I',r,a&0x1fffff)[0]
gp=0x8013D83C
base=R(gp+6356)&0x1fffff
sizes={0x20:20,0x24:32,0x28:24,0x2c:40,0x30:28,0x34:40,0x38:36,0x3c:52,0x60:16,0x64:20,0x68:12,0x70:12,0x74:16,0x78:12,0x7c:16,0x40:16,0x50:20}
o=base; n=0
while n<200:
    tag=R(0x80000000+o); code=r[o+7]
    k=code&0xfc
    if code in (0xe1,0xe2,0xe3,0xe4,0xe5,0xe6):
        print(f'{o-base:5d} {code:02x} env {R(0x80000000+o+4):08x}'); o+=8; n+=1; continue
    if k not in sizes:
        print(f'{o-base:5d} ?? {r[o:o+16].hex()}'); o+=4; n+=1; continue
    sz=sizes[k]; d=r[o:o+sz]
    rgb=d[4:7].hex()
    if k in (0x64,0x74,0x7c,0x60,0x68,0x70,0x78):
        x,y=struct.unpack_from('<hh',d,8)
        extra=''
        if k in (0x64,0x74,0x7c):
            u,v,clut=struct.unpack_from('<BBH',d,12); extra=f'uv=({u},{v}) clut={clut:04x}'
            if k==0x64: extra+=' wh=%s'%(struct.unpack_from('<hh',d,16),)
        elif k==0x60: extra='wh=%s'%(struct.unpack_from('<hh',d,12),)
        print(f'{o-base:5d} {code:02x} SPRT/TILE rgb={rgb} xy=({x},{y}) {extra}')
    elif k in (0x2c,0x3c,0x24,0x34):
        if k==0x2c: pts=[struct.unpack_from('<hh',d,8+8*i) for i in range(4)]; uvs=[d[12+8*i:14+8*i] for i in range(4)]; tp=struct.unpack_from('<H',d,22)[0]; clut=struct.unpack_from('<H',d,14)[0]
        elif k==0x3c: pts=[struct.unpack_from('<hh',d,8+12*i) for i in range(4)]; uvs=[d[12+12*i:14+12*i] for i in range(4)]; tp=struct.unpack_from('<H',d,26)[0]; clut=struct.unpack_from('<H',d,14)[0]
        else: pts=[];uvs=[];tp=0;clut=0
        print(f'{o-base:5d} {code:02x} POLY_T rgb={rgb} pts={pts} uv={[tuple(u) for u in uvs]} tpage={tp:04x} clut={clut:04x}')
    else:
        if k==0x28: pts=[struct.unpack_from('<hh',d,8+4*i) for i in range(4)]
        elif k==0x38: pts=[struct.unpack_from('<hh',d,8+8*i) for i in range(4)]
        else: pts=[]
        print(f'{o-base:5d} {code:02x} POLY rgb={rgb} pts={pts}')
    o+=sz; n+=1
