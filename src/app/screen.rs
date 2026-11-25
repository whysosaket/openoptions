use eframe::egui::{Pos2, Rect, Vec2, pos2, vec2};

pub(super) fn overlay_origin(pointer: Pos2, screen: Option<Rect>, scale: f32, edge: f32) -> Pos2 {
    let scale = scale.max(f32::EPSILON);
    let edge = Vec2::splat(edge);
    let desired = pointer / scale - edge / 2.;
    let Some(screen) = screen else {
        return desired;
    };
    let min = screen.min / scale;
    let max = screen.max / scale - edge;
    pos2(
        desired.x.clamp(min.x, max.x.max(min.x)),
        desired.y.clamp(min.y, max.y.max(min.y)),
    )
}

#[cfg(target_os = "macos")]
pub(super) fn containing_screen(pointer: (i32, i32)) -> Option<Rect> {
    #[repr(C)]
    struct Point {
        x: f64,
        y: f64,
    }

    #[repr(C)]
    struct Size {
        width: f64,
        height: f64,
    }

    #[repr(C)]
    struct DisplayRect {
        origin: Point,
        size: Size,
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGGetDisplaysWithPoint(
            point: Point,
            max_displays: u32,
            displays: *mut u32,
            count: *mut u32,
        ) -> i32;
        fn CGDisplayBounds(display: u32) -> DisplayRect;
    }

    let mut display = 0;
    let mut count = 0;
    let point = Point {
        x: f64::from(pointer.0),
        y: f64::from(pointer.1),
    };
    // The output pointers each refer to one initialized slot.
    let result = unsafe { CGGetDisplaysWithPoint(point, 1, &mut display, &mut count) };
    if result != 0 || count == 0 {
        return None;
    }
    let bounds = unsafe { CGDisplayBounds(display) };
    Some(Rect::from_min_size(
        pos2(bounds.origin.x as f32, bounds.origin.y as f32),
        vec2(bounds.size.width as f32, bounds.size.height as f32),
    ))
}

#[cfg(target_os = "windows")]
pub(super) fn containing_screen(pointer: (i32, i32)) -> Option<Rect> {
    use windows_sys::Win32::{
        Foundation::POINT,
        Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint},
    };

    let point = POINT {
        x: pointer.0,
        y: pointer.1,
    };
    // The result is an opaque monitor handle owned by Windows.
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_null() {
        return None;
    }
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..MONITORINFO::default()
    };
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return None;
    }
    let bounds = info.rcWork;
    Some(Rect::from_min_max(
        pos2(bounds.left as f32, bounds.top as f32),
        pos2(bounds.right as f32, bounds.bottom as f32),
    ))
}

#[cfg(target_os = "linux")]
pub(super) fn containing_screen(pointer: (i32, i32)) -> Option<Rect> {
    use x11rb::{connection::Connection, protocol::xinerama::ConnectionExt};

    let (connection, screen) = x11rb::connect(None).ok()?;
    let point = pos2(pointer.0 as f32, pointer.1 as f32);
    if let Some(reply) = connection
        .xinerama_query_screens()
        .ok()
        .and_then(|cookie| cookie.reply().ok())
    {
        for screen in reply.screen_info {
            let rect = Rect::from_min_size(
                pos2(f32::from(screen.x_org), f32::from(screen.y_org)),
                vec2(f32::from(screen.width), f32::from(screen.height)),
            );
            if rect.contains(point) {
                return Some(rect);
            }
        }
    }
    let root = connection.setup().roots.get(screen)?;
    Some(Rect::from_min_size(
        Pos2::ZERO,
        vec2(
            f32::from(root.width_in_pixels),
            f32::from(root.height_in_pixels),
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_clamps_on_the_display_with_a_negative_origin() {
        let display = Rect::from_min_size(pos2(-1440., 0.), vec2(1440., 900.));
        assert_eq!(
            overlay_origin(pos2(-1438., 898.), Some(display), 1., 288.),
            pos2(-1440., 612.)
        );
        assert_eq!(
            overlay_origin(pos2(-10., 22.), Some(display), 2., 200.),
            pos2(-200., 0.)
        );
    }
}
