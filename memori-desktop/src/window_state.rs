use tauri::{LogicalPosition, LogicalSize, Position, Size};

use crate::{
    AppSettings, DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH, MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH,
    load_app_settings, save_app_settings,
};

pub(crate) fn restore_main_window_state(
    window: &tauri::WebviewWindow,
    settings: &AppSettings,
) -> Result<(), String> {
    let has_saved_size = match (settings.window_width, settings.window_height) {
        (Some(w), Some(h)) => w >= MIN_WINDOW_WIDTH && h >= MIN_WINDOW_HEIGHT,
        _ => false,
    };
    let has_saved_position = match (settings.window_x, settings.window_y) {
        (Some(x), Some(y)) => x > -10_000 && y > -10_000,
        _ => false,
    };

    let monitor = window
        .current_monitor()
        .map_err(|err| format!("read current monitor failed: {err}"))?
        .or_else(|| window.primary_monitor().ok().flatten());

    let Some(monitor) = monitor else {
        let target_w = settings
            .window_width
            .unwrap_or(DEFAULT_WINDOW_WIDTH)
            .max(MIN_WINDOW_WIDTH);
        let target_h = settings
            .window_height
            .unwrap_or(DEFAULT_WINDOW_HEIGHT)
            .max(MIN_WINDOW_HEIGHT);
        set_logical_size(window, target_w, target_h)?;
        if has_saved_position {
            if let (Some(x), Some(y)) = (settings.window_x, settings.window_y) {
                let _ = set_logical_position(window, x, y);
            }
        } else {
            let _ = window.center();
        }
        if settings.window_maximized.unwrap_or(false) {
            let _ = window.maximize();
        }
        return Ok(());
    };

    let scale_factor = monitor.scale_factor().max(1.0);
    let monitor_pos = monitor.position();
    let monitor_size = monitor.size();
    let monitor_x = physical_i32_to_logical(monitor_pos.x, scale_factor);
    let monitor_y = physical_i32_to_logical(monitor_pos.y, scale_factor);
    let monitor_w = physical_u32_to_logical(monitor_size.width, scale_factor);
    let monitor_h = physical_u32_to_logical(monitor_size.height, scale_factor);
    let max_w = monitor_w.saturating_sub(32);
    let max_h = monitor_h.saturating_sub(64);

    let target_w = settings
        .window_width
        .unwrap_or(DEFAULT_WINDOW_WIDTH)
        .clamp(MIN_WINDOW_WIDTH, max_w.max(MIN_WINDOW_WIDTH));
    let target_h = settings
        .window_height
        .unwrap_or(DEFAULT_WINDOW_HEIGHT)
        .clamp(MIN_WINDOW_HEIGHT, max_h.max(MIN_WINDOW_HEIGHT));

    let max_x = monitor_x.saturating_add(monitor_w as i32 - target_w as i32);
    let max_y = monitor_y.saturating_add(monitor_h as i32 - target_h as i32);
    let fallback_x = monitor_x.saturating_add((monitor_w as i32 - target_w as i32) / 2);
    let fallback_y = monitor_y.saturating_add((monitor_h as i32 - target_h as i32) / 2);

    let target_x = if has_saved_position {
        settings
            .window_x
            .unwrap_or(fallback_x)
            .clamp(monitor_x, max_x.max(monitor_x))
    } else {
        fallback_x
    };
    let target_y = if has_saved_position {
        settings
            .window_y
            .unwrap_or(fallback_y)
            .clamp(monitor_y, max_y.max(monitor_y))
    } else {
        fallback_y
    };

    set_logical_size(window, target_w, target_h)?;
    set_logical_position(window, target_x, target_y)?;

    if !has_saved_size && !has_saved_position {
        let _ = window.center();
    }

    if settings.window_maximized.unwrap_or(false) {
        let _ = window.maximize();
    }

    Ok(())
}

pub(crate) fn persist_main_window_state(window: &tauri::Window) -> Result<(), String> {
    let mut settings = load_app_settings()?;
    let minimized = window
        .is_minimized()
        .map_err(|err| format!("read minimized state failed: {err}"))?;
    if minimized {
        return Ok(());
    }
    let maximized = window
        .is_maximized()
        .map_err(|err| format!("read maximized state failed: {err}"))?;

    settings.window_maximized = Some(maximized);
    if !maximized {
        let scale_factor = window
            .scale_factor()
            .map_err(|err| format!("read window scale factor failed: {err}"))?
            .max(1.0);
        let size = window
            .outer_size()
            .map_err(|err| format!("read window size failed: {err}"))?;
        let pos = window
            .outer_position()
            .map_err(|err| format!("read window position failed: {err}"))?;
        if pos.x <= -10_000 || pos.y <= -10_000 {
            return Ok(());
        }

        let logical_width = physical_u32_to_logical(size.width, scale_factor);
        let logical_height = physical_u32_to_logical(size.height, scale_factor);
        let logical_x = physical_i32_to_logical(pos.x, scale_factor);
        let logical_y = physical_i32_to_logical(pos.y, scale_factor);
        if logical_width < MIN_WINDOW_WIDTH || logical_height < MIN_WINDOW_HEIGHT {
            return Ok(());
        }
        settings.window_width = Some(logical_width);
        settings.window_height = Some(logical_height);
        settings.window_x = Some(logical_x);
        settings.window_y = Some(logical_y);
    }

    save_app_settings(&settings)
}

fn set_logical_size(window: &tauri::WebviewWindow, width: u32, height: u32) -> Result<(), String> {
    window
        .set_size(Size::Logical(LogicalSize::new(width as f64, height as f64)))
        .map_err(|err| format!("set window size failed: {err}"))
}

fn set_logical_position(window: &tauri::WebviewWindow, x: i32, y: i32) -> Result<(), String> {
    window
        .set_position(Position::Logical(LogicalPosition::new(x as f64, y as f64)))
        .map_err(|err| format!("set window position failed: {err}"))
}

fn physical_u32_to_logical(value: u32, scale_factor: f64) -> u32 {
    (value as f64 / scale_factor).round().max(0.0) as u32
}

fn physical_i32_to_logical(value: i32, scale_factor: f64) -> i32 {
    (value as f64 / scale_factor).round() as i32
}
