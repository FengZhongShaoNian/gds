//! Spatial transform wrapper and inverse coordinate mapping for interactive cropping.

#[cfg(test)]
mod tests;

use gds::service::image::{
    PixelRect,
    ops::{OrthogonalRotation, TransformSpec},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransformState {
    pub spec: TransformSpec,
}

impl TransformState {
    pub fn output_dimensions(self, base_w: u32, base_h: u32) -> (u32, u32) {
        let (w, h) = self
            .spec
            .crop
            .and_then(|crop| crop.clamp_to(base_w, base_h))
            .map_or((base_w.max(1), base_h.max(1)), |rect| {
                (rect.width, rect.height)
            });
        let (rw, rh) = if self.spec.rotation.swaps_axes() {
            (h, w)
        } else {
            (w, h)
        };
        self.spec.resize.unwrap_or((rw, rh))
    }

    pub fn is_identity(self) -> bool {
        self.spec == TransformSpec::default()
    }

    /// Converts a selection rectangle drawn on the currently transformed canvas back into
    /// unrotated base image pixel coordinates for non-destructive cropping.
    pub fn map_output_rect_to_base_crop(
        self,
        output_rect: PixelRect,
        base_w: u32,
        base_h: u32,
    ) -> Option<PixelRect> {
        if base_w == 0 || base_h == 0 {
            return None;
        }
        let domain = self
            .spec
            .crop
            .and_then(|crop| crop.clamp_to(base_w, base_h))
            .unwrap_or_else(|| PixelRect::new(0, 0, base_w, base_h));
        let (rot_w, rot_h) = if self.spec.rotation.swaps_axes() {
            (domain.height, domain.width)
        } else {
            (domain.width, domain.height)
        };
        if rot_w == 0 || rot_h == 0 {
            return None;
        }
        let (out_w, out_h) = self.spec.resize.unwrap_or((rot_w, rot_h));
        if out_w == 0 || out_h == 0 {
            return None;
        }
        let clamped_out = output_rect.clamp_to(out_w, out_h)?;

        let pre_resize = if (out_w, out_h) == (rot_w, rot_h) {
            clamped_out
        } else {
            rescale_rect(clamped_out, (out_w, out_h), (rot_w, rot_h))?
        };

        let mut rx = pre_resize.x;
        let mut ry = pre_resize.y;
        let rw = pre_resize.width;
        let rh = pre_resize.height;

        if self.spec.flip_v {
            ry = rot_h.saturating_sub(ry.saturating_add(rh));
        }
        if self.spec.flip_h {
            rx = rot_w.saturating_sub(rx.saturating_add(rw));
        }

        let (dx, dy, dw, dh) = match self.spec.rotation {
            OrthogonalRotation::Deg0 => (rx, ry, rw, rh),
            OrthogonalRotation::Deg90 => (
                ry,
                domain.height.saturating_sub(rx.saturating_add(rw)),
                rh,
                rw,
            ),
            OrthogonalRotation::Deg180 => (
                domain.width.saturating_sub(rx.saturating_add(rw)),
                domain.height.saturating_sub(ry.saturating_add(rh)),
                rw,
                rh,
            ),
            OrthogonalRotation::Deg270 => (
                domain.width.saturating_sub(ry.saturating_add(rh)),
                rx,
                rh,
                rw,
            ),
        };

        PixelRect::new(
            domain.x.saturating_add(dx),
            domain.y.saturating_add(dy),
            dw,
            dh,
        )
        .clamp_to(base_w, base_h)
    }
}

fn rescale_rect(rect: PixelRect, from: (u32, u32), to: (u32, u32)) -> Option<PixelRect> {
    let (fw, fh) = (u64::from(from.0), u64::from(from.1));
    let (tw, th) = (u64::from(to.0), u64::from(to.1));
    let x0 = (u64::from(rect.x) * tw / fw) as u32;
    let y0 = (u64::from(rect.y) * th / fh) as u32;
    let x1 = ((u64::from(rect.x + rect.width) * tw).div_ceil(fw)).min(tw) as u32;
    let y1 = ((u64::from(rect.y + rect.height) * th).div_ceil(fh)).min(th) as u32;
    PixelRect::new(x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0)).clamp_to(to.0, to.1)
}
