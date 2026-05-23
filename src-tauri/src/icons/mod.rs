//! Inline SVG glyphs and the rasteriser that turns them into RGBA
//! buffers Tauri / AppKit can consume.
//!
//! The tray gets the SVG rasterised at 44x44 (22pt @2x) and applied
//! through Tauri's `Image::new_owned`; macOS template mode handles
//! the recolour per menubar appearance.
//!
//! The dock icon gets the same SVG rasterised at 1024x1024, wrapped
//! into a coloured rounded square, and pushed at runtime through
//! `NSApplication.setApplicationIconImage:`. The bundle PNGs in
//! `icons/` are still the default Tauri-CLI placeholder; this
//! runtime override is what the user sees in the Dock / Cmd+Tab
//! while the app is running.

/// Lucide `mic` glyph. The colour values here are placeholder —
/// the tray treats the alpha channel as a template (macOS recolours
/// per menubar theme) and the dock build replaces the stroke before
/// rasterising.
pub const MIC_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 19v3"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><rect x="9" y="2" width="6" height="13" rx="3"/></svg>"##;

/// Build the tray icon as a 44x44 RGBA buffer. 22pt @2x stays crisp
/// on Retina menubars; macOS reads only the alpha so a pure black
/// stroke is fine.
pub fn build_tray_icon_rgba() -> (Vec<u8>, u32, u32) {
    const SIZE: u32 = 44;
    rasterise_svg(MIC_SVG, SIZE)
}

/// Build the dock icon as a 1024x1024 PNG byte buffer with a soft
/// violet background and the mic glyph centred in white. Returned as
/// PNG so the macOS bridge can pass it straight to `NSImage`.
pub fn build_dock_icon_png() -> Vec<u8> {
    const SIZE: u32 = 1024;
    // Tint the glyph the same violet used as `--accent` on light theme
    // so the dock icon reads as part of the koko family.
    let coloured_svg = MIC_SVG.replace("stroke=\"black\"", "stroke=\"#ffffff\"");
    let (glyph_rgba, _, _) = rasterise_svg(&coloured_svg, SIZE);

    // Composite: violet rounded square background + glyph on top.
    let mut canvas = vec![0u8; (SIZE * SIZE * 4) as usize];
    paint_rounded_violet_background(&mut canvas, SIZE);
    overlay_centered(&mut canvas, SIZE, &glyph_rgba, SIZE, 0.55);

    encode_png(&canvas, SIZE)
}

/// Pure. Rasterise an SVG string into a square RGBA buffer at the
/// given pixel size. Assumes the SVG viewBox is 24x24 (Lucide
/// default).
fn rasterise_svg(svg: &str, size: u32) -> (Vec<u8>, u32, u32) {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(svg, &opt).expect("parse svg");
    let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("alloc pixmap");
    let scale = size as f32 / 24.0;
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    (pixmap.take(), size, size)
}

/// Fill `canvas` with a violet rounded-square background. Mutates in
/// place; the canvas is `size * size * 4` bytes of pre-zeroed RGBA.
fn paint_rounded_violet_background(canvas: &mut [u8], size: u32) {
    // RGB pulled from --accent: rgba(76, 29, 149, 1). Slightly
    // brighter (167, 139, 250) read better at thumbnail scale, so
    // gradient from the saturated end to the lighter end top-down.
    let top = (124u8, 58u8, 237u8); // violet-600
    let bottom = (76u8, 29u8, 149u8); // violet-900
    let radius = (size as f32) * 0.22;
    let r2 = radius * radius;
    let s = size as f32;

    for y in 0..size {
        let t = y as f32 / s;
        let r = ((1.0 - t) * top.0 as f32 + t * bottom.0 as f32) as u8;
        let g = ((1.0 - t) * top.1 as f32 + t * bottom.1 as f32) as u8;
        let b = ((1.0 - t) * top.2 as f32 + t * bottom.2 as f32) as u8;
        for x in 0..size {
            // Approximate rounded-corner clip: only the four corners
            // need the distance check; the centre rectangle is
            // unconditionally filled.
            let in_corner = (x as f32 <= radius || x as f32 >= s - radius)
                && (y as f32 <= radius || y as f32 >= s - radius);
            let inside = if in_corner {
                let cx = if (x as f32) < radius {
                    radius
                } else {
                    s - radius
                };
                let cy = if (y as f32) < radius {
                    radius
                } else {
                    s - radius
                };
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                dx * dx + dy * dy <= r2
            } else {
                true
            };
            let idx = ((y * size + x) * 4) as usize;
            if inside {
                canvas[idx] = r;
                canvas[idx + 1] = g;
                canvas[idx + 2] = b;
                canvas[idx + 3] = 255;
            } else {
                canvas[idx + 3] = 0;
            }
        }
    }
}

/// Overlay `glyph` (RGBA at `glyph_size`) onto `canvas` (RGBA at
/// `canvas_size`), scaled to `scale` of the canvas and centred. Uses
/// straight alpha compositing because the glyph buffer is already
/// straight-alpha from resvg.
fn overlay_centered(
    canvas: &mut [u8],
    canvas_size: u32,
    glyph: &[u8],
    glyph_size: u32,
    scale: f32,
) {
    let target_size = ((canvas_size as f32) * scale) as i32;
    let offset_x = ((canvas_size as i32) - target_size) / 2;
    let offset_y = ((canvas_size as i32) - target_size) / 2;
    for y in 0..target_size {
        let sy = (y as f32 / target_size as f32 * glyph_size as f32) as u32;
        for x in 0..target_size {
            let sx = (x as f32 / target_size as f32 * glyph_size as f32) as u32;
            let src_idx = ((sy * glyph_size + sx) * 4) as usize;
            let cx = offset_x + x;
            let cy = offset_y + y;
            if cx < 0 || cy < 0 || cx >= canvas_size as i32 || cy >= canvas_size as i32 {
                continue;
            }
            let dst_idx = ((cy as u32 * canvas_size + cx as u32) * 4) as usize;
            let sa = glyph[src_idx + 3] as f32 / 255.0;
            if sa <= 0.0 {
                continue;
            }
            let inv = 1.0 - sa;
            canvas[dst_idx] = ((glyph[src_idx] as f32) * sa
                + (canvas[dst_idx] as f32) * inv) as u8;
            canvas[dst_idx + 1] = ((glyph[src_idx + 1] as f32) * sa
                + (canvas[dst_idx + 1] as f32) * inv) as u8;
            canvas[dst_idx + 2] = ((glyph[src_idx + 2] as f32) * sa
                + (canvas[dst_idx + 2] as f32) * inv) as u8;
            canvas[dst_idx + 3] =
                (255.0 * sa + canvas[dst_idx + 3] as f32 * inv).min(255.0) as u8;
        }
    }
}

/// Encode an RGBA buffer as PNG. The dock-icon path needs PNG bytes
/// because `NSImage.initWithData:` takes a serialised image format.
fn encode_png(rgba: &[u8], size: u32) -> Vec<u8> {
    let out = Vec::with_capacity((size * size) as usize);
    let mut cursor = std::io::Cursor::new(out);
    let encoder = image::codecs::png::PngEncoder::new(&mut cursor);
    use image::ImageEncoder;
    encoder
        .write_image(rgba, size, size, image::ExtendedColorType::Rgba8)
        .expect("encode png");
    cursor.into_inner()
}

#[cfg(target_os = "macos")]
pub fn apply_dock_icon() {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSData;
    let png = build_dock_icon_png();
    unsafe {
        let app: *mut AnyObject = msg_send![objc2::class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let data = NSData::with_bytes(&png);
        let ns_image: *mut AnyObject = msg_send![objc2::class!(NSImage), alloc];
        let ns_image: *mut AnyObject = msg_send![ns_image, initWithData: &*data];
        if ns_image.is_null() {
            return;
        }
        let _: () = msg_send![app, setApplicationIconImage: ns_image];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn apply_dock_icon() {}
