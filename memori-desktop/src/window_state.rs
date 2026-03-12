use tauri::{PhysicalPosition, PhysicalSize, Position, Size};

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
        .map_err(|err| format!("读取当前显示器失败: {err}"))?
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
        window
            .set_size(Size::Physical(PhysicalSize::new(target_w, target_h)))
            .map_err(|err| format!("设置窗口尺寸失败: {err}"))?;
        if has_saved_position {
            if let (Some(x), Some(y)) = (settings.window_x, settings.window_y) {
                let _ = window.set_position(Position::Physical(PhysicalPosition::new(x, y)));
            }
        } else {
            let _ = window.center();
        }
        if settings.window_maximized.unwrap_or(false) {
            let _ = window.maximize();
        }
        return Ok(());
    };

    let monitor_pos = monitor.position();
    let monitor_size = monitor.size();
    let max_w = monitor_size.width.saturating_sub(32);
    let max_h = monitor_size.height.saturating_sub(64);
    let default_w = ((monitor_size.width as f32) * 0.9).round() as u32;
    let default_h = ((monitor_size.height as f32) * 0.88).round() as u32;

    let target_w = settings
        .window_width
        .unwrap_or_else(|| DEFAULT_WINDOW_WIDTH.max(default_w))
        .clamp(MIN_WINDOW_WIDTH, max_w.max(MIN_WINDOW_WIDTH));
    let target_h = settings
        .window_height
        .unwrap_or_else(|| DEFAULT_WINDOW_HEIGHT.max(default_h))
        .clamp(MIN_WINDOW_HEIGHT, max_h.max(MIN_WINDOW_HEIGHT));

    let max_x = monitor_pos
        .x
        .saturating_add(monitor_size.width as i32 - target_w as i32);
    let max_y = monitor_pos
        .y
        .saturating_add(monitor_size.height as i32 - target_h as i32);

    let fallback_x = monitor_pos
        .x
        .saturating_add((monitor_size.width as i32 - target_w as i32) / 2);
    let fallback_y = monitor_pos
        .y
        .saturating_add((monitor_size.height as i32 - target_h as i32) / 2);

    let target_x = if has_saved_position {
        settings
            .window_x
            .unwrap_or(fallback_x)
            .clamp(monitor_pos.x, max_x.max(monitor_pos.x))
    } else {
        fallback_x
    };
    let target_y = if has_saved_position {
        settings
            .window_y
            .unwrap_or(fallback_y)
            .clamp(monitor_pos.y, max_y.max(monitor_pos.y))
    } else {
        fallback_y
    };

    window
        .set_size(Size::Physical(PhysicalSize::new(target_w, target_h)))
        .map_err(|err| format!("设置窗口尺寸失败: {err}"))?;
    window
        .set_position(Position::Physical(PhysicalPosition::new(
            target_x, target_y,
        )))
        .map_err(|err| format!("设置窗口位置失败: {err}"))?;

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
        .map_err(|err| format!("读取窗口最小化状态失败: {err}"))?;
    if minimized {
        return Ok(());
    }
    let maximized = window
        .is_maximized()
        .map_err(|err| format!("读取窗口最大化状态失败: {err}"))?;

    settings.window_maximized = Some(maximized);
    if !maximized {
        let size = window
            .outer_size()
            .map_err(|err| format!("读取窗口尺寸失败: {err}"))?;
        let pos = window
            .outer_position()
            .map_err(|err| format!("读取窗口位置失败: {err}"))?;
        if pos.x <= -10_000 || pos.y <= -10_000 {
            return Ok(());
        }
        if size.width < MIN_WINDOW_WIDTH || size.height < MIN_WINDOW_HEIGHT {
            return Ok(());
        }
        settings.window_width = Some(size.width);
        settings.window_height = Some(size.height);
        settings.window_x = Some(pos.x);
        settings.window_y = Some(pos.y);
    }

    save_app_settings(&settings)
}
