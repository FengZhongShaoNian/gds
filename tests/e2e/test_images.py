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


def test_I11_move_annotation_and_repeat_delete_safety(case):
    original = case.sample_image('original.png', (320, 200))
    case.launch('images', case.path('original.png'))
    case.expect('original.png', 'image')
    target = case.image_target(original)
    case.key('4')
    case.drag_image(target, (40, 30), (120, 90))
    # Drag on the top stroke away from NW (40, 30) and N (80, 30) handles to translate (+100, +50).
    case.drag_image(target, (60, 30), (160, 80))
    case.key('ctrl+c')
    case.expect('Copied image to clipboard')
    with case.step('Verify moved rectangle pixels in guest Wayland clipboard'):
        encoded = case.desktop.call('clipboard', mime='image/png')
        copied = Image.open(io.BytesIO(base64.b64decode(encoded))).convert('RGBA')
        assert copied.size == original.size
        changes = ImageChops.difference(copied.convert('RGB'), original.convert('RGB'))
        assert changes.crop((35, 25, 75, 35)).getbbox() is None
        assert changes.crop((35, 25, 125, 95)).getbbox() is None
        assert changes.crop((135, 75, 225, 145)).getbbox() is not None
        assert changes.crop((175, 75, 185, 86)).getbbox() is not None
        assert changes.crop((135, 105, 146, 115)).getbbox() is not None
        assert changes.crop((170, 105, 190, 115)).getbbox() is None
        assert changes.crop((0, 0, 320, 65)).getbbox() is None
        assert changes.crop((0, 155, 320, 200)).getbbox() is None
    case.key('delete')
    case.key('delete')
    case.key('delete')
    case.key('ctrl+s')
    case.expect(contains='Saved ', role='status bar')
    case.expect_file('original.png', exists=True)
    case.expect_empty_trash()
    case.expect_image('original.png', original)


def test_I12_rotate_and_crop_preserve_annotations(case):
    original = case.sample_image('original.png', (320, 200))
    case.launch('images', case.path('original.png'))
    case.expect('original.png', 'image')
    target = case.image_target(original)
    case.key('4')
    case.drag_image(target, (60, 40), (160, 120))
    case.key('escape')
    case.key('escape')
    case.key('r')
    case.expect(contains='200 × 320')
    case.key('ctrl+s')
    case.expect(contains='Saved ', role='status bar')
    with case.step('Verify rotated rectangle strokes and unchanged background'):
        expected_base = original.transpose(Image.Transpose.ROTATE_270).convert('RGB')
        saved = Image.open(io.BytesIO(case.desktop.read(case.path('original.png')))).convert('RGB')
        assert saved.size == (200, 320)
        changes = ImageChops.difference(saved, expected_base)
        bounds = changes.getbbox()
        assert bounds is not None, 'Rotated annotation must change pixels'
        assert 74 <= bounds[0] <= 85 and 54 <= bounds[1] <= 65
        assert 155 <= bounds[2] <= 166 and 155 <= bounds[3] <= 166
        assert changes.crop((115, 55, 125, 66)).getbbox() is not None
        assert changes.crop((75, 105, 86, 115)).getbbox() is not None
        assert changes.crop((95, 75, 145, 145)).getbbox() is None
        assert changes.crop((0, 0, 70, 320)).getbbox() is None
        assert changes.crop((170, 0, 200, 320)).getbbox() is None
    case.quit()
    crop_src = case.coordinate_image('crop-source.png')
    case.launch('images', case.path('crop-source.png'))
    case.expect('crop-source.png', 'image')
    crop_target = case.image_target(crop_src)
    case.key('4')
    case.drag_image(crop_target, (60, 40), (160, 120))
    case.key('escape')
    case.key('3')
    case.drag_image(crop_target, (90, 20), (240, 160))
    case.activate('Apply Crop')
    case.expect('Apply Crop', present=False)
    width, height = case.image_dimensions()
    assert 148 <= width <= 152 and 138 <= height <= 142
    case.key('ctrl+c')
    case.expect('Copied image to clipboard')
    with case.step('Verify cropped clipboard retains clipped rectangle without border artifacts'):
        encoded = case.desktop.call('clipboard', mime='image/png')
        cropped = Image.open(io.BytesIO(base64.b64decode(encoded))).convert('RGBA')
        assert cropped.size == (width, height)
        left, top, blue, _ = cropped.getpixel((0, 0))
        assert blue == 123 and 89 <= left <= 91 and 19 <= top <= 21
        expected_crop = crop_src.crop((left, top, left + width, top + height)).convert('RGB')
        crop_changes = ImageChops.difference(cropped.convert('RGB'), expected_crop)
        assert crop_changes.crop((20, 40 - top - 5, 45, 40 - top + 6)).getbbox() is not None
        assert crop_changes.crop((160 - left - 5, 50, 160 - left + 6, 70)).getbbox() is not None
        assert crop_changes.crop((20, 120 - top - 5, 45, 120 - top + 6)).getbbox() is not None
        assert crop_changes.crop((0, 40 - top + 8, 6, 120 - top - 8)).getbbox() is None
        assert crop_changes.crop((15, 40 - top + 8, 160 - left - 10, 120 - top - 8)).getbbox() is None
        assert crop_changes.crop((160 - left + 8, 0, width, height)).getbbox() is None
    case.key('ctrl+z')
    case.expect(contains='320 × 200')
    case.key('ctrl+s')
    case.expect(contains='Saved ', role='status bar')
    with case.step('Verify crop undo restores complete rectangle'):
        restored = Image.open(io.BytesIO(case.desktop.read(case.path('crop-source.png')))).convert('RGB')
        assert restored.size == (320, 200)
        restored_changes = ImageChops.difference(restored, crop_src.convert('RGB'))
        assert restored_changes.crop((55, 70, 66, 90)).getbbox() is not None
        assert restored_changes.crop((155, 70, 166, 90)).getbbox() is not None
        assert restored_changes.crop((75, 55, 145, 105)).getbbox() is None
