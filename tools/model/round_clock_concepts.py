"""Original round-clock concept previews; documentation-only, not production code."""
import math, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hanglock_ref import Canvas, smooth, write_png
from hanglock_concepts import bar, draw_text

CONCEPTS = {
    "minimal": dict(radius=78, hang=62, cord=1.8, body=(.075,.09,.12,.97), rim=(.48,.66,.82,.55),
                     shadow=(.0,.0,.0,.20), ink=(.86,.92,.98), suffix=(.64,.75,.86), accent=(.20,.48,.72),
                     mount=(.28,.35,.43), bg=(.90,.89,.86)),
    "premium": dict(radius=88, hang=76, cord=1.35, body=(.12,.14,.19,.88), rim=(.58,.70,.86,.72),
                     shadow=(.0,.0,.0,.18), ink=(.94,.96,1.0), suffix=(.68,.79,.92), accent=(.46,.70,.90),
                     mount=(.38,.45,.55), bg=(.045,.055,.075)),
    "compact": dict(radius=64, hang=48, cord=1.25, body=(.055,.065,.075,.98), rim=(.65,.70,.72,.48),
                     shadow=(.0,.0,.0,.14), ink=(.88,.91,.90), suffix=(.61,.70,.68), accent=(.36,.60,.55),
                     mount=(.25,.29,.31), bg=(.93,.93,.91)),
}

def circle(cv,cx,cy,r,col,aa=1.0):
    R,G,B,A=col; x0=max(0,int(cx-r-2)); x1=min(cv.w-1,int(cx+r+2)); y0=max(0,int(cy-r-2)); y1=min(cv.h-1,int(cy+r+2))
    for y in range(y0,y1+1):
        for x in range(x0,x1+1):
            d=math.hypot(x+.5-cx,y+.5-cy)-r
            a=smooth(aa*.5,-aa*.5,d)*A
            if a>0.002: cv.over(y*cv.w+x,R,G,B,a); cv.touch(x,y)

def ring(cv,cx,cy,r,w,col):
    circle(cv,cx,cy,r,col); circle(cv,cx,cy,r-w,(col[0],col[1],col[2],0.0))

def one(path,name):
    k=CONCEPTS[name]; r=k['radius']; ax=170; ay=20; cx=ax; cy=ay+k['hang']+r; w=340; h=int(cy+r+42); cv=Canvas(w,h)
    # background is only for preview presentation, not an app surface
    circle(cv,ax,ay+2,12,(*k['mount'],.94)); bar(cv,ax-22,ay+1,ax+22,ay+1,2.4,(*k['mount'],.96),end=0.25)
    bar(cv,ax,ay+5,cx,cy-r+3,k['cord'],(*k['accent'],.92),end=.5)
    circle(cv,cx,cy+5,r+9,(k['shadow'][0],k['shadow'][1],k['shadow'][2],.12)); circle(cv,cx,cy+4,r+5,(k['shadow'][0],k['shadow'][1],k['shadow'][2],.10))
    circle(cv,cx,cy,r,k['body']); ring(cv,cx,cy,r-1.5,1.8,k['rim'])
    # eyelet is visibly part of the clock, with the cord terminating in it
    circle(cv,cx,cy-r+3,7,(*k['mount'],.95)); circle(cv,cx,cy-r+3,3,(.02,.025,.03,.98))
    text='10:42'; cap=min(r*.42,(r*1.55)/(len(text)*.96)); tw=len(text)*cap*.96
    draw_text(cv,text,cx-tw/2,cy-cap*.43,cap,(*k['ink'],1),weight=.92)
    draw_text(cv,'PM',cx+tw/2+cap*.12,cy-cap*.18,cap*.30,(*k['suffix'],.95),weight=.9)
    # tiny accent tick at six o'clock gives the face a designed datum without copying a dial
    bar(cv,cx,cy+r*.72,cx,cy+r*.82,1.0,(*k['accent'],.65),end=.35)
    write_png(path,cv.w,cv.h,cv.over_solid(k['bg']),3)

def main(out):
    os.makedirs(out,exist_ok=True)
    for n in CONCEPTS: one(os.path.join(out,'round-'+n+'.png'),n)
    print('round previews ->',out)
if __name__=='__main__': main(sys.argv[1] if len(sys.argv)>1 else 'docs/previews/phase-2.5b-round')
