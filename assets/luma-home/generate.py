#!/usr/bin/env python3
"""Rebuild Luma Home's editable SVG identity and PNG exports.

Requires fonttools and rsvg-convert. Uses the repository's OFL Inter fonts;
wordmarks are outlined paths, so exported SVGs require no installed font.
The icon geometry is a deliberate manual reconstruction of the imagegen concept.
"""
from pathlib import Path
from html import escape
import subprocess
from fontTools.ttLib import TTFont
from fontTools.pens.svgPathPen import SVGPathPen

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
BLACK, GRAPHITE, WHITE, MINT = '#000000', '#24262B', '#F5F6F7', '#A5F0D4'
FONTS = {True: TTFont(ROOT / 'pkg/appfont-bold.ttf'), False: TTFont(ROOT / 'pkg/appfont.ttf')}


def mark(ink=WHITE, accent=MINT):
    return f'''<g fill="none" stroke="{ink}" stroke-width="36" stroke-linecap="round" stroke-linejoin="round">
<path d="M316 128 H172 Q128 128 128 172 V340 Q128 384 172 384 H340 Q384 384 384 340 V212"/>
<path d="M196 208 V324 H288"/></g><circle cx="376" cy="134" r="27" fill="{accent}"/>'''


def text(value, x, y, size, ink=WHITE, bold=False):
    font = FONTS[bold]
    glyphs = font.getGlyphSet()
    cmap = font.getBestCmap()
    scale = size / font['head'].unitsPerEm
    parts = []
    advance = 0
    for char in value:
        name = cmap.get(ord(char), '.notdef')
        pen = SVGPathPen(glyphs)
        glyphs[name].draw(pen)
        command = pen.getCommands()
        if command:
            parts.append(f'<path d="{command}" transform="translate({x + advance * scale:.3f},{y}) scale({scale:.6f},-{scale:.6f})"/>')
        advance += font['hmtx'].metrics[name][0]
    return f'<g fill="{ink}" aria-label="{escape(value)}">' + ''.join(parts) + '</g>'


def svg(width, height, body, title):
    return f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img"><title>{escape(title)}</title>{body}</svg>\n'


def write(name, width, height, body, title):
    (HERE / f'{name}.svg').write_text(svg(width, height, body, title))


def lockup(ink=WHITE, accent=MINT):
    return (f'<g transform="translate(0,0) scale(0.38)">{mark(ink, accent)}</g>'
            + text('Luma Home', 220, 128, 96, ink, True)
            + text('Native TV dashboard', 224, 188, 30, ink))


write('mark', 512, 512, mark(), 'Luma Home mark')
write('mark-mono-light', 512, 512, mark(WHITE, WHITE), 'Luma Home monochrome light mark')
write('mark-mono-dark', 512, 512, mark(BLACK, BLACK), 'Luma Home monochrome dark mark')
write('icon', 512, 512, f'<rect width="512" height="512" rx="112" fill="{BLACK}"/>' + mark(), 'Luma Home app icon')
write('wordmark-dark', 900, 220, lockup(), 'Luma Home wordmark for dark backgrounds')
write('wordmark-light', 900, 220, lockup(BLACK, BLACK), 'Luma Home monochrome wordmark for light backgrounds')
write('banner', 1600, 480, f'<rect width="1600" height="480" fill="{BLACK}"/><g transform="translate(260,120)">{lockup()}</g>', 'Luma Home native TV dashboard')

board = f'<rect width="1600" height="960" fill="{BLACK}"/>'
board += f'<g transform="translate(70,65) scale(0.82)">{mark()}</g>'
board += f'<g transform="translate(590,145) scale(1.0)">{lockup()}</g>'
board += text('OLED black canvas. Graphite surfaces. Quiet focus.', 105, 540, 32)
board += f'<g transform="translate(86,630) scale(0.34)">{mark(WHITE, WHITE)}</g>'
board += f'<rect x="340" y="622" width="178" height="178" rx="38" fill="{WHITE}"/><g transform="translate(343,625) scale(0.34)">{mark(BLACK, BLACK)}</g>'
board += text('Monochrome light', 90, 853, 24)
board += text('Monochrome dark', 342, 853, 24)
for x, colour, label in [(705, BLACK, 'OLED black'), (920, GRAPHITE, 'Graphite'), (1135, WHITE, 'Soft white'), (1350, MINT, 'Mint')]:
    board += f'<circle cx="{x}" cy="710" r="64" fill="{colour}" stroke="{WHITE if colour == BLACK else colour}" stroke-width="1"/>'
    board += text(label, x - 70, 820, 24)
    board += text(colour.upper(), x - 70, 858, 22)
write('brand-board', 1600, 960, board, 'Luma Home OLED-first identity — vector reconstruction')

for name in ['icon', 'banner', 'brand-board']:
    subprocess.run(['rsvg-convert', '-o', str(HERE / f'{name}.png'), str(HERE / f'{name}.svg')], check=True)
for size in [32, 64, 128, 160, 512]:
    subprocess.run(['rsvg-convert', '-w', str(size), '-h', str(size), '-o', str(HERE / f'icon-{size}.png'), str(HERE / 'icon.svg')], check=True)
