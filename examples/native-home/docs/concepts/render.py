#!/usr/bin/env python3
"""Draw editable synthetic design concepts; no TV connection or app code runs.

PNG rendering requires rsvg-convert. The temporary Fontconfig file points only to
the repository's existing OFL Inter fonts and is never included in the artwork.
"""
from pathlib import Path
from html import escape
import os
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
BG, PANEL, INK, MUTED, HAIRLINE = '#000000', '#222224', '#f7fafc', '#b8bfcc', '#48484c'
ACCENT, DARK = INK, '#08080a'
ART = ['#394852', '#54473d', '#404650', '#514454', '#415049', '#524e40']
SIZES = {'display': 48, 'title': 40, 'headline': 32, 'body': 28, 'label': 26, 'caption': 24}


class Canvas:
    def __init__(self):
        self.parts = [f'<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080"><title>Luma Home design mockup — synthetic data</title><rect width="1920" height="1080" fill="{BG}"/>']

    def rect(self, x, y, w, h, fill=PANEL, radius=24, stroke=None, sw=1):
        extra = f' stroke="{stroke}" stroke-width="{sw}"' if stroke else ''
        self.parts.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{radius}" fill="{fill}"{extra}/>')

    def text(self, x, y, content, size='body', colour=INK, weight=400, anchor='start'):
        self.parts.append(f'<text x="{x}" y="{y}" font-family="Inter,sans-serif" font-size="{SIZES[size]}" font-weight="{weight}" fill="{colour}" text-anchor="{anchor}">{escape(content)}</text>')

    def line(self, x1, y1, x2, y2, colour=HAIRLINE, width=1):
        self.parts.append(f'<path d="M{x1} {y1} L{x2} {y2}" stroke="{colour}" stroke-width="{width}" fill="none"/>')

    def icon(self, kind, x, y, size=64, colour=INK):
        paths = {
            'home': '<path d="M8 29L32 9L56 29M14 25V55H50V25M27 55V37H37V55"/>',
            'apps': '<rect x="9" y="9" width="18" height="18" rx="4"/><rect x="37" y="9" width="18" height="18" rx="4"/><rect x="9" y="37" width="18" height="18" rx="4"/><rect x="37" y="37" width="18" height="18" rx="4"/>',
            'input': '<rect x="6" y="13" width="52" height="34" rx="5"/><path d="M22 55H42M32 47V55M23 25L38 25M32 19L38 25L32 31"/>',
            'controls': '<path d="M10 15H54M10 32H54M10 49H54"/><circle cx="23" cy="15" r="6"/><circle cx="43" cy="32" r="6"/><circle cx="23" cy="49" r="6"/>',
            'play': '<path d="M24 15L48 32L24 49Z"/>',
            'library': '<rect x="9" y="12" width="13" height="42" rx="3"/><rect x="27" y="12" width="13" height="42" rx="3"/><path d="M47 14L59 51L49 54L37 17Z"/>',
            'music': '<path d="M25 45V16L51 10V39M25 21L51 15"/><ellipse cx="17" cy="47" rx="8" ry="6"/><ellipse cx="43" cy="41" rx="8" ry="6"/>',
            'photos': '<rect x="8" y="8" width="48" height="48" rx="7"/><circle cx="24" cy="23" r="5"/><path d="M10 48L26 32L36 42L45 31L56 43"/>',
            'search': '<circle cx="27" cy="27" r="16"/><path d="M40 40L55 55"/>',
            'games': '<path d="M17 24H47C59 24 63 53 53 53L41 43H23L11 53C1 53 5 24 17 24Z"/><path d="M17 30V42M11 36H23"/><circle cx="43" cy="33" r="2"/><circle cx="50" cy="39" r="2"/>',
            'star': '<path d="M32 6L40 24L59 26L45 40L49 59L32 49L15 59L19 40L5 26L24 24Z"/>',
            'lock': '<rect x="15" y="28" width="34" height="29" rx="5"/><path d="M22 28V18C22 4 42 4 42 18V28M32 39V46"/>',
        }
        self.parts.append(f'<g transform="translate({x},{y}) scale({size/64})" fill="none" stroke="{colour}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">{paths[kind]}</g>')

    def pill(self, x, y, label, width, selected=False):
        self.rect(x, y, width, 52, ACCENT if selected else PANEL, 26)
        self.text(x+width/2, y+35, label, 'caption', DARK if selected else MUTED, 600 if selected else 400, 'middle')

    def header(self, section):
        self.text(96, 112, 'Luma Home', 'display', weight=600)
        self.text(96, 163, section, 'label', MUTED)
        self.text(1824, 112, '19:42', 'display', weight=500, anchor='end')
        self.text(1824, 163, 'Wednesday, 14 October', 'caption', MUTED, anchor='end')
        self.line(96, 200, 1824, 200)
        names = [('Home','home'),('Apps','apps'),('Inputs','input'),('Controls','controls')]
        for i, (name, icon) in enumerate(names):
            y=264+i*84
            if name == section:
                self.rect(96,y-10,286,68,'#38383c',18)
                self.rect(96,y+8,4,32,ACCENT,2)
            self.icon(icon,120,y+7,32,INK if name == section else MUTED)
            self.text(176,y+35,name,'headline',INK if name == section else MUTED,600 if name == section else 400)
        self.line(120,670,358,670)
        self.icon('home',120,698,28,MUTED)
        self.text(164,722,'LG Home','caption',MUTED)
        self.text(120,848,'TV connected','caption',MUTED)
        self.text(120,887,'Sound · TV speaker','caption',MUTED)

    def footer(self, hint):
        self.line(96,978,1824,978)
        self.text(96,1026,hint,'caption',MUTED)
        self.text(1824,1026,'DESIGN MOCKUP · SYNTHETIC DATA','caption',MUTED,anchor='end')

    def app(self,x,y,title,kind,index,w=300,h=234,focused=False,pinned=True):
        self.rect(x,y,w,h,ART[index%len(ART)],28,ACCENT if focused else '#626269',3 if focused else 1)
        self.icon(kind,x+w/2-42,y+h/2-42,84)
        if pinned:
            self.icon('star',x+w-43,y+18,24)
        self.text(x+w/2,y+h+42,title,'body',weight=600 if focused else 400,anchor='middle')

    def save(self,name):
        (HERE/f'{name}.svg').write_text('\n'.join(self.parts)+'</svg>\n')


def home():
    c=Canvas();c.header('Home')
    c.text(480,288,'Your favourites','title',weight=600)
    c.text(1824,284,'View all apps  ›','caption',MUTED,anchor='end')
    for i,(title,kind) in enumerate([('Library','library'),('Cinema','play'),('Music','music'),('Photos','photos')]):
        c.app(480+i*344,326,title,kind,i,312,228,i==0)
    c.text(480,686,'Inputs','headline',weight=600)
    c.text(1824,682,'View all inputs  ›','caption',MUTED,anchor='end')
    for i,(title,status) in enumerate([('Games console','Active · Signal detected'),('Media player','Connected · No signal'),('HDMI 3','Not connected'),('HDMI 4','Not connected')]):
        x=480+i*344;c.rect(x,722,312,190)
        c.icon('input',x+24,746,44,INK if i==0 else MUTED)
        c.text(x+24,822,title,'label',weight=500)
        c.text(x+24,865,f'HDMI {i+1}','caption',MUTED)
        c.text(x+24,895,status,'caption',MUTED)
    c.footer('OK  Open   ·   Hold OK  Manage favourite   ·   Back  Exit')
    c.save('home')


def apps():
    c=Canvas();c.header('Apps')
    c.rect(480,248,778,62);c.icon('search',502,263,32,MUTED)
    c.text(552,289,'Search installed apps','label',MUTED)
    c.pill(480,336,'All apps',142);c.pill(638,336,'Favourites',180)
    c.rect(480,336,142,52,'#3b3b40',26)
    c.text(551,371,'All apps','caption',INK,600,'middle')
    c.text(1258,371,'A–Z  ↓','caption',MUTED,anchor='end')
    for i,(title,kind) in enumerate([('Cinema','play'),('Games','games'),('Library','library'),('Music','music'),('Photos','photos'),('Radio','music')]):
        c.app(480+(i%3)*270,424+(i//3)*264,title,kind,i,238,180,False,i in (0,2,3,4))
    c.rect(1020,424,238,180,'none',28,MUTED,2)
    c.rect(1302,248,522,698,PANEL,28)
    c.icon('library',1342,284,68)
    c.text(1342,401,'Library','title',weight=600)
    c.text(1342,449,'Favourite application','caption',MUTED)
    c.line(1342,484,1784,484)
    c.text(1342,546,'Open application','body')
    c.rect(1326,580,474,72,ACCENT,18)
    c.text(1350,626,'Remove from favourites','label',DARK,600)
    c.text(1342,712,'Move in favourites  ›','label')
    c.text(1342,796,'Pinning and ordering change','caption',MUTED)
    c.text(1342,831,'only your dashboard layout.','caption',MUTED)
    c.footer('Arrows  Navigate   ·   OK  Select   ·   Back  Close menu')
    c.save('apps')


def inputs():
    c=Canvas();c.header('Inputs')
    c.text(480,288,'Choose a source','title',weight=600)
    tiles=[('Games console','HDMI 1','Signal detected'),('Media player','HDMI 2','Connected · No signal'),('HDMI 3','HDMI 3','Not connected'),('HDMI 4','HDMI 4','Not connected')]
    for i,(title,port,status) in enumerate(tiles):
        x=480+(i%2)*406;y=330+(i//2)*304
        c.rect(x,y,372,272,ART[0] if i==0 else PANEL,28,HAIRLINE,1)
        c.icon('games' if i==0 else 'input',x+28,y+24,68,INK if i<2 else MUTED)
        if i==0:c.pill(x+224,y+30,'Active',118)
        c.text(x+28,y+151,title,'headline',weight=500)
        c.text(x+28,y+198,port,'caption',MUTED)
        c.text(x+28,y+237,status,'caption',MUTED)
    c.rect(1326,330,498,576,PANEL,28)
    c.text(1366,395,'Games console','headline',weight=600)
    for j,(label,value) in enumerate([('Input','HDMI 1'),('Connection','Plugged in'),('Signal','Detected'),('Video format','3840 × 2160 · 60 Hz')]):
        y=456+j*80;c.text(1366,y,label,'caption',MUTED);c.text(1366,y+35,value,'label')
    c.rect(1350,796,450,72,ACCENT,18)
    c.text(1575,842,'Open HDMI 1','label',DARK,600,'middle')
    c.footer('OK  Open source   ·   Back  Home')
    c.save('inputs')


def controls():
    c=Canvas();c.header('Controls')
    c.text(480,286,'Control centre','title',weight=600)
    cats=['Picture','Sound','Network','Accessibility','General','Panel Care']
    for i,name in enumerate(cats):
        y=334+i*74
        if i==0:c.rect(480,y,282,62,'#3b3b40',16)
        c.text(504,y+41,name,'label',INK if i==0 else MUTED,600 if i==0 else 400)
    c.rect(798,326,1026,620,PANEL,28)
    c.text(838,388,'Picture','headline',weight=600)
    c.text(838,432,'HDMI 1  /  SDR  /  Cinema','caption',MUTED)
    rows=[('Picture mode','Cinema  ›'),('OLED pixel brightness','70'),('Contrast','85'),('Black level','50')]
    for i,(label,value) in enumerate(rows):
        y=466+i*78
        if i==1:c.rect(820,y,982,68,ACCENT,18)
        col=DARK if i==1 else INK
        c.text(846,y+43,label,'label',col,600 if i==1 else 400)
        c.text(1776,y+43,value,'label',col,600 if i==1 else 400,'end')
    c.line(838,793,1784,793)
    c.icon('lock',838,819,30,MUTED)
    c.text(882,842,'OLED protections stay managed by LG','caption',MUTED)
    c.text(838,901,'More picture settings','label')
    c.text(1784,901,'Open LG Settings  ›','label',anchor='end')
    c.footer('OK  Adjust verified control   ·   Back  Home')
    c.save('controls')


def overview():
    parts=[f'<svg xmlns="http://www.w3.org/2000/svg" width="2560" height="1660" viewBox="0 0 2560 1660"><title>Luma Home four-screen concept overview</title><rect width="2560" height="1660" fill="#141416"/>']
    parts.append(f'<text x="64" y="71" font-family="Inter,sans-serif" font-size="40" font-weight="600" fill="{INK}">Luma Home · Interface direction</text>')
    parts.append(f'<text x="2496" y="68" font-family="Inter,sans-serif" font-size="24" fill="{MUTED}" text-anchor="end">Design mockups · Synthetic data</text>')
    for i,name in enumerate(('home','apps','inputs','controls')):
        x=64+(i%2)*1248;y=150+(i//2)*752
        parts.append(f'<text x="{x}" y="{y-20}" font-family="Inter,sans-serif" font-size="28" font-weight="500" fill="{INK}">{i+1:02} · {name.capitalize()}</text>')
        document=(HERE/f'{name}.svg').read_text()
        inside=document[document.index('>')+1:document.rindex('</svg>')]
        parts.append(f'<g transform="translate({x},{y}) scale(0.625)">{inside}</g>')
    (HERE/'overview.svg').write_text('\n'.join(parts)+'</svg>\n')


if __name__ == '__main__':
    for draw in (home,apps,inputs,controls):draw()
    overview()
    with tempfile.TemporaryDirectory(prefix='native-home-concepts-') as temp:
        config=Path(temp)/'fonts.conf'
        config.write_text(f'<fontconfig><dir>{escape(str(ROOT/"pkg"))}</dir><cachedir>{escape(temp)}</cachedir></fontconfig>')
        env={**os.environ,'FONTCONFIG_FILE':str(config)}
        for name in ('home','apps','inputs','controls','overview'):
            subprocess.run(['rsvg-convert',str(HERE/f'{name}.svg'),'-o',str(HERE/f'{name}.png')],check=True,env=env)
    print('Wrote four editable SVG/1920×1080 PNG concepts and a 2560×1660 overview.')
