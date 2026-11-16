"""Images workflows verify exported pixels with Pillow, independent of GDS rendering."""

import base64
import io

from PIL import Image, ImageChops
import pytest

from driver import eventually

pytestmark = pytest.mark.images


def test_I01_gallery_navigation_and_panels(case):
    original = case.sample_image('01-first.png')
    case.sample_image('02-second.png', (60, 90))
    case.launch('images', case.path('01-first.png'))
    case.expect('01-first.png', 'image')
    case.expect(contains='80 × 48')
    case.key('right')
    case.expect('02-second.png', 'image')
    case.expect(contains='60 × 90')
    case.key('left')
    case.expect('01-first.png', 'image')
    case.key('ctrl+i')
    case.expect_state('Tools & Properties', 'pressed', False)
    case.key('ctrl+b')
    case.expect_state('Toggle Sidebar', 'pressed', False)
    case.key('ctrl+1')
    case.expect('01-first.png', 'image')
    case.expect_image('01-first.png', original)
    case.checkpoint('gallery')


def test_I02_rotate_undo_redo_save_and_reopen(case):
    original = case.sample_image('original.png')
    case.launch('images', case.path('original.png'))
    case.expect('original.png', 'image')
    case.key('r')
    case.expect(contains='48 × 80')
    case.key('ctrl+z')
    case.expect(contains='80 × 48')
    case.key('ctrl+shift+z')
    case.expect(contains='48 × 80')
    case.key('ctrl+s')
    case.expect_image('original.png', original.transpose(Image.Transpose.ROTATE_270))
    case.quit()
    case.launch('images', case.path('original.png'))
    case.expect('original.png', 'image')
    case.expect(contains='48 × 80')


def test_I03_flip_save_preserves_exact_pixels(case):
    original = case.sample_image('original.png')
    case.launch('images', case.path('original.png'))
    case.expect('original.png', 'image')
    case.key('h')
    case.key('ctrl+s')
    case.expect_image('original.png', original.transpose(Image.Transpose.FLIP_LEFT_RIGHT))


def test_I04_copy_image_to_real_desktop_clipboard(case):
    original = case.sample_image('original.png')
    case.launch('images', case.path('original.png'))
    case.expect('original.png', 'image')
    case.key('ctrl+c')
    case.expect('Copied image to clipboard')
    with case.step('Read image/png through guest Wayland clipboard'):
        encoded = case.desktop.call('clipboard', mime='image/png')
        copied = Image.open(io.BytesIO(base64.b64decode(encoded))).convert('RGBA')
        assert copied.size == original.size
        assert copied.tobytes() == original.tobytes()


def test_I05_stdin_capture_save_and_exit(case):
    original = case.sample_image('capture-source.png')
    case.launch('images', '--stdin', '--output', case.path('saved.png'), '--early-exit',
                stdin=case.path('capture-source.png'))
    case.expect(role='image')
    case.key('ctrl+s')
    case.expect_image('saved.png', original)
    eventually(lambda: case.desktop.call('alive', pid=case.pid), lambda alive: not alive,
               'capture exits after save')
    case.expect_exit(0)
    case.expect_image('capture-source.png', original)


def test_I06_rename_image_preserves_contents(case):
    original = case.sample_image('original.png')
    case.launch('images', case.path('original.png'))
    case.expect('original.png', 'image')
    case.key('ctrl+l')
    case.expect_entry()
    case.key('ctrl+a')
    case.type('renamed.png')
    case.key('enter')
    case.expect('renamed.png', 'image')
    case.expect_image('renamed.png', original)
    case.expect_file('original.png', exists=False)


def test_I07_external_change_blocks_overwrite(case):
    case.sample_image('original.png')
    case.launch('images', case.path('original.png'))
    case.expect('original.png', 'image')
    case.key('r')
    case.expect(contains='48 × 80')
    external = case.sample_image('original.png', (30, 20))
    case.key('ctrl+s')
    case.expect(contains='was modified externally', role='notification')
    case.expect_image('original.png', external)


def test_I08_settings_return_to_document(case):
    original = case.sample_image('original.png')
    case.launch('images', case.path('original.png'))
    case.expect('original.png', 'image')
    case.key('ctrl+comma')
    case.expect('Viewer')
    case.activate('Export & Privacy')
    case.activate('Appearance')
    case.key('escape')
    case.expect('original.png', 'image')
    case.expect_image('original.png', original)


def test_I09_crop_with_real_pointer_and_save(case):
    original = case.coordinate_image('original.png')
    case.launch('images', case.path('original.png'))
    case.expect('original.png', 'image')
    target = case.image_target(original)
    case.key('3')
    case.drag_image(target, (40, 30), (240, 150))
    case.activate('Apply Crop')
    case.expect('Apply Crop', present=False)
    width, height = case.image_dimensions()
    # Absolute tablet coordinates can straddle a pixel boundary by less than one pixel.
    assert 198 <= width <= 202 and 118 <= height <= 122
    case.key('ctrl+s')
    case.expect(contains='Saved ', role='status bar')
    cropped = Image.open(io.BytesIO(case.desktop.read(case.path('original.png')))).convert('RGBA')
    left, top, _, _ = cropped.getpixel((0, 0))
    assert 39 <= left <= 41 and 29 <= top <= 31
    assert 239 <= left + width <= 241 and 149 <= top + height <= 151
    case.expect_image('original.png', original.crop((left, top, left + width, top + height)))


def test_I10_rectangle_annotation_is_exported(case):
    original = case.sample_image('original.png', (320, 200))
    case.launch('images', case.path('original.png'))
    case.expect('original.png', 'image')
    target = case.image_target(original)
    case.key('4')
    case.drag_image(target, (40, 30), (240, 150))
    case.key('ctrl+s')
    case.expect(contains='Saved ', role='status bar')
    with case.step('Verify annotation pixels and unchanged surrounding content'):
        saved = Image.open(io.BytesIO(case.desktop.read(case.path('original.png')))).convert('RGB')
        assert saved.size == original.size
        changes = ImageChops.difference(saved, original.convert('RGB'))
        bounds = changes.getbbox()
        assert bounds is not None, 'Saved annotation must change pixels'
        assert 30 <= bounds[0] <= 45 and 20 <= bounds[1] <= 35
        assert 235 <= bounds[2] <= 250 and 145 <= bounds[3] <= 160
        assert changes.crop((135, 25, 145, 35)).getbbox() is not None
        assert changes.crop((35, 85, 45, 95)).getbbox() is not None
        assert changes.crop((130, 80, 150, 100)).getbbox() is None
        assert changes.crop((0, 0, 320, 15)).getbbox() is None
        assert changes.crop((0, 170, 320, 200)).getbbox() is None
