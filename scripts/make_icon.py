"""生成 flashasr 全部图标（用法：py -3 scripts/make_icon.py）。

flashasr 图标：品牌蓝圆角方块，左侧声波竖条 → 右侧文字横线（语音 → 文字 = 语音识别服务）。"""
import os
import tempfile

from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__))) + os.sep
# 预览图写到系统临时目录（不入库）
OUT_PREVIEW = os.path.join(tempfile.gettempdir(), 'flashasr-icon-preview.png')
S = 1024
SS = 4  # 超采样


def master() -> Image.Image:
    n = S * SS
    img = Image.new('RGBA', (n, n), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    pad = int(n * 0.04)
    # 背景：左上天青 → 右下靛紫的斜向渐变 + 圆角（Gavin 10-30「高级时尚蓝色，不是传统单调沉闷的蓝色」）
    top, bot = (0x36, 0xC2, 0xFF), (0x4F, 0x46, 0xE5)
    small = 256
    grad = Image.new('RGBA', (small, small))
    for y in range(small):
        for x in range(small):
            t = (x + y) / (2 * (small - 1))
            grad.putpixel((x, y), tuple(int(top[i] + (bot[i] - top[i]) * t) for i in range(3)) + (255,))
    grad = grad.resize((n, n), Image.BICUBIC)
    mask = Image.new('L', (n, n), 0)
    ImageDraw.Draw(mask).rounded_rectangle([pad, pad, n - pad, n - pad], radius=int(n * 0.22), fill=255)
    img.paste(grad, (0, 0), mask)

    white = (255, 255, 255, 255)
    u = n / 1024
    bar_w = 64 * u
    cy = n / 2
    # 声波竖条（左）
    xs = [200, 296, 392, 488]
    hs = [230, 470, 330, 170]
    for x, h in zip(xs, hs):
        x0 = x * u
        d.rounded_rectangle([x0, cy - h * u / 2, x0 + bar_w, cy + h * u / 2], radius=bar_w / 2, fill=white)
    # 文字横线（右）
    lx = 610 * u
    lines = [(395, 250), (512, 250), (629, 165)]
    for y, length in lines:
        yy = y * u
        d.rounded_rectangle([lx, yy - bar_w / 2, lx + length * u, yy + bar_w / 2], radius=bar_w / 2, fill=white)
    return img.resize((S, S), Image.LANCZOS)


def sized(m: Image.Image, px: int) -> Image.Image:
    return m.resize((px, px), Image.LANCZOS)


m = master()
icons = ROOT + 'src-tauri/icons/'
m.save(icons + 'icon-source.png')
for px, name in [(16, '16x16.png'), (32, '32x32.png'), (128, '128x128.png'), (256, '128x128@2x.png'),
                 (256, '256x256.png'), (512, '512x512.png'), (16, 'tray-16x16.png')]:
    sized(m, px).save(icons + name)
sized(m, 1024).save(icons + 'tray-source.png')
ico_sizes = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
m.save(icons + 'icon.ico', sizes=ico_sizes)
m.save(ROOT + 'assets/icons/app.ico', sizes=ico_sizes)
try:
    m.save(icons + 'icon.icns')
    icns = 'ok'
except Exception as e:  # noqa: BLE001
    icns = f'icns failed: {e}'

# 预览：各尺寸并排（浅底 + 深底）
sizes = [16, 24, 32, 48, 64, 128, 256]
W = sum(sizes) + 20 * (len(sizes) + 1)
H = 256 * 2 + 60
pv = Image.new('RGBA', (W, H), (245, 245, 245, 255))
ImageDraw.Draw(pv).rectangle([0, H // 2, W, H], fill=(40, 40, 40, 255))
x = 20
for px in sizes:
    ic = sized(m, px)
    pv.paste(ic, (x, 20 + 256 - px), ic)
    pv.paste(ic, (x, H // 2 + 10 + 256 - px), ic)
    x += px + 20
pv.save(OUT_PREVIEW)
print('ok', icns)
