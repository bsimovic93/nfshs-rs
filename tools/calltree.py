import sys; sys.path.insert(0,'.')
import mips
e=mips.Exe('../gamedata/NFS4.EXE')
def bounds(s):
    p=s+4
    while e.valid(p) and not ((e.word(p)>>16)==0x27bd and (e.word(p)&0x8000)): p+=4
    # leaf functions without prologue: stop at first jr ra + delay
    q=s
    while q<p:
        if e.word(q)==0x03e00008 and ((e.word(s)>>16)!=0x27bd): return s,q+8
        q+=4
    return s,p
def calls(s):
    s,p=bounds(s); out=[]
    for q in range(s,p,4):
        w=e.word(q)
        if w>>26==3: out.append((q&0xf0000000)|((w&0x3ffffff)<<2))
    return sorted(set(out)), (p-s)//4
def tree(root, skip=()):
    seen={}; stack=[root]
    while stack:
        f=stack.pop()
        if f in seen or f in skip: continue
        c,n=calls(f); seen[f]=(n,c); stack+=c
    return seen
if __name__=='__main__':
    for r in sys.argv[1:]:
        t=tree(int(r,16))
        print(r, 'functions', len(t), 'instrs', sum(v[0] for v in t.values()), 'direct', [hex(x) for x in calls(int(r,16))[0]], 'own', calls(int(r,16))[1])
