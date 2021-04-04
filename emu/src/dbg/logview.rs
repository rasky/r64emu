use super::uisupport::{is_shortcut_pressed, ImGuiListClipper};
use super::{LogViewCommand, UiCtxLog};
use crate::log::{LogPool, LogPoolPtr, LogView};
use sdl2::keyboard::Scancode;

use imgui::*;
use slog::LOG_LEVEL_SHORT_NAMES;
use textwrap;
use tinyfiledialogs::save_file_dialog_with_filter;

use std::time::Instant;

const LOG_LEVEL_COLOR: [[f32; 4]; 7] = [
    [255.0 / 255.0, 255.0 / 255.0, 255.0 / 255.0, 255.0], // none
    [231.0 / 255.0, 50.0 / 255.0, 50.0 / 255.0, 255.0],   // critical
    [231.0 / 255.0, 50.0 / 255.0, 50.0 / 255.0, 255.0],   // error
    [230.0 / 255.0, 219.0 / 255.0, 116.0 / 255.0, 255.0], // warning
    [165.0 / 255.0, 224.0 / 255.0, 46.0 / 255.0, 255.0],  // info
    [255.0 / 255.0, 255.0 / 255.0, 255.0 / 255.0, 255.0], // debug
    [102.0 / 255.0, 99.0 / 255.0, 83.0 / 255.0, 255.0],   // trace
];

const COLOR_MODULE: [f32; 4] = [174.0 / 129.0, 129.0 / 255.0, 255.0 / 255.0, 255.0];
const COLOR_FRAME: [f32; 4] = [95.0 / 129.0, 158.0 / 255.0, 160.0 / 255.0, 255.0];

fn render_filter_by_levels(ui: &Ui, view: &mut LogView) -> bool {
    let mut update_filter = false;
    let levels: [&ImStr; 7] = [
        im_str!("Off (ALL)"),
        im_str!("Critical (CRIT)"),
        im_str!("Error (ERRO)"),
        im_str!("Warning (WARN)"),
        im_str!("Info (INFO)"),
        im_str!("Debug (DEBG)"),
        im_str!("Trace (TRCE)"),
    ];
    let mut max_level = match view.filter_max_level() {
        None => 0,
        Some(ml) => ml as usize,
    };
    ui.set_next_item_width(130.0);
    if ComboBox::new(im_str!("##level")).build_simple_string(&ui, &mut max_level, &levels) {
        view.set_filter_max_level(if max_level == 0 {
            None
        } else {
            Some(slog::FilterLevel::from_usize(max_level).unwrap())
        });
        update_filter = true;
    }
    if ui.is_item_hovered() {
        ui.tooltip_text(im_str!("Minimum displayed level"));
    }
    update_filter
}

fn render_filter_by_modules(ui: &Ui, view: &mut LogView, pool: &LogPool) -> bool {
    let mut update_filter = false;
    if ui.button(im_str!("Modules.."), [0.0, 0.0]) {
        ui.open_popup(im_str!("##modules"));
    }
    if ui.is_item_hovered() {
        ui.tooltip_text(im_str!("Show/hide specific modules"));
    }
    ui.popup(im_str!("##modules"), || {
        if ui.button(im_str!("Show all"), [ui.window_content_region_width(), 0.0]) {
            view.set_filter_modules(None);
            ui.close_current_popup();
            update_filter = true;
        }
        ui.separator();

        let mut selected = Vec::with_capacity(pool.modules.len());
        if let Some(ref modules) = view.filter_modules() {
            selected.resize(pool.modules.len(), false);
            for m in modules.iter() {
                selected[*m as usize] = true;
            }
        } else {
            selected.resize(pool.modules.len(), true);
        }

        let mut changed = false;
        for (n, m) in pool.modules.iter().enumerate() {
            if ui.checkbox(&im_str!("{}", m), &mut selected[n]) {
                changed = true;
            }
        }
        if changed {
            let numsel = selected.iter().filter(|v| **v).count();
            view.set_filter_modules(if numsel == selected.len() {
                None
            } else {
                Some(
                    selected
                        .iter()
                        .enumerate()
                        .filter_map(|(idx, v)| Some(idx as u8).filter(|_| *v))
                        .collect(),
                )
            });
            update_filter = true;
        }
    });
    update_filter
}

#[derive(Copy, Clone, PartialEq)]
enum RangeType {
    None = 0,
    Since = 1,
    First = 2,
    Single = 3,
    Interval = 4,
}

impl From<usize> for RangeType {
    fn from(val: usize) -> RangeType {
        match val {
            0 => RangeType::None,
            1 => RangeType::Since,
            2 => RangeType::First,
            3 => RangeType::Single,
            4 => RangeType::Interval,
            _ => unreachable!(),
        }
    }
}

fn render_filter_by_frame(ui: &Ui, view: &mut LogView, num_frames: i64) -> bool {
    let mut update_filter = false;
    let num_frames = num_frames as i32;
    if ui.button(im_str!("Frames.."), [0.0, 0.0]) {
        ui.open_popup(im_str!("##frames"));
    }
    if ui.is_item_hovered() {
        ui.tooltip_text(im_str!("Show/hide specific frame ranges"));
    }
    ui.popup(im_str!("##frames"), || {
        ui.separator();

        let mut range_type = match (view.filter_min_frame(), view.filter_max_frame()) {
            (None, None) => RangeType::None,
            (Some(_), None) => RangeType::Since,
            (None, Some(_)) => RangeType::First,
            (Some(f1), Some(f2)) if f1 == f2 => RangeType::Single,
            (Some(f1), Some(f2)) if f1 != f2 => RangeType::Interval,
            _ => unreachable!(),
        };

        let any_frame_first = view
            .filter_min_frame()
            .unwrap_or(view.filter_max_frame().unwrap_or(0));
        let mut any_frame_last = view
            .filter_max_frame()
            .unwrap_or(view.filter_min_frame().unwrap_or(0));

        let mut range_val = range_type as usize;
        if ComboBox::new(im_str!("##cb")).build_simple_string(
            ui,
            &mut range_val,
            &[
                im_str!("Show all"),
                im_str!("Since Nth frame"),
                im_str!("Up to Nth frame"),
                im_str!("Single frame"),
                im_str!("Specific interval"),
            ],
        ) {
            range_type = range_val.into();
            match range_type {
                RangeType::None => {
                    view.set_filter_min_frame(None);
                    view.set_filter_max_frame(None);
                }
                RangeType::Since => {
                    view.set_filter_min_frame(Some(any_frame_first));
                    view.set_filter_max_frame(None);
                }
                RangeType::First => {
                    view.set_filter_min_frame(None);
                    view.set_filter_max_frame(Some(any_frame_last));
                }
                RangeType::Single => {
                    view.set_filter_min_frame(Some(any_frame_first));
                    view.set_filter_max_frame(Some(any_frame_first));
                }
                RangeType::Interval => {
                    if any_frame_first == any_frame_last {
                        any_frame_last += 1;
                    }
                    view.set_filter_min_frame(Some(any_frame_first));
                    view.set_filter_max_frame(Some(any_frame_last));
                }
            }
            update_filter = true;
        }

        match range_type {
            RangeType::None => {}
            RangeType::Since => {
                let mut val = view.filter_min_frame().unwrap() as i32;
                if ui
                    .drag_int(im_str!("##last"), &mut val)
                    .min(0)
                    .max(num_frames)
                    .display_format(im_str!("Frames: %d"))
                    .build()
                {
                    view.set_filter_min_frame(Some(val as u32));
                    update_filter = true;
                }
            }
            RangeType::First => {
                let mut val = view.filter_max_frame().unwrap() as i32;
                if ui
                    .drag_int(im_str!("##first"), &mut val)
                    .min(0)
                    .max(num_frames)
                    .display_format(im_str!("Frames: %d"))
                    .build()
                {
                    view.set_filter_max_frame(Some(val as u32));
                    update_filter = true;
                }
            }
            RangeType::Single => {
                let mut val = view.filter_min_frame().unwrap() as i32;
                if ui
                    .drag_int(im_str!("##single"), &mut val)
                    .min(0)
                    .max(num_frames)
                    .display_format(im_str!("Frame: %d"))
                    .build()
                {
                    view.set_filter_min_frame(Some(val as u32));
                    view.set_filter_max_frame(Some(val as u32));
                    update_filter = true;
                }
            }
            RangeType::Interval => {
                let mut val1 = view.filter_min_frame().unwrap() as i32;
                let mut val2 = view.filter_max_frame().unwrap() as i32;
                if ui
                    .drag_int(im_str!("##start"), &mut val1)
                    .min(0)
                    .max(val2 - 1)
                    .display_format(im_str!("Start: %d"))
                    .build()
                {
                    view.set_filter_min_frame(Some(val1 as u32));
                    update_filter = true;
                }
                if ui
                    .drag_int(im_str!("##end"), &mut val2)
                    .min(val1 + 1)
                    .max(num_frames)
                    .display_format(im_str!("End: %d"))
                    .build()
                {
                    view.set_filter_max_frame(Some(val2 as u32));
                    update_filter = true;
                }
            }
        }
        if range_type != RangeType::None {
            ui.text_disabled("Hold SHIFT to change faster");
        }

        ui.separator();
        if ui.button(im_str!("Close"), [ui.window_content_region_width(), 0.0]) {
            ui.close_current_popup();
        }
    });
    update_filter
}

fn render_filter_by_text(ui: &Ui, view: &mut LogView) -> bool {
    let mut update_filter = false;
    let mut text_filter = ImString::with_capacity(64);
    if let Some(ref text) = view.filter_text() {
        text_filter.push_str(&text);
    }
    if InputText::new(ui, im_str!("##textfilter"), &mut text_filter)
        .auto_select_all(true)
        //.hint(im_str!("search..."))   // FIXME: not implemented yet
        .build()
    {
        view.set_filter_text(if text_filter.to_str().len() == 0 {
            None
        } else {
            Some(text_filter.to_str())
        });
        update_filter = true;
    }
    if ui.is_item_hovered() {
        ui.tooltip_text(im_str!("Search within logs"));
    }
    update_filter
}

fn render_save<'a, 'ui>(ui: &'a Ui<'ui>, view: &mut LogView, pool: &LogPool) {
    if ui.button(im_str!("Save.."), [0.0, 0.0]) {
        if let Some(path) = save_file_dialog_with_filter(
            "Save log file",
            ".",
            &vec![".log"],
            "Save all the logs to disk",
        ) {
            view.save(&path, pool).unwrap();
        }
    }
    if ui.is_item_hovered() {
        ui.tooltip_text(im_str!("Save logs to disk (respecting current filters)"));
    }
}

pub(crate) fn render_logview<'a, 'ui>(
    ui: &'a Ui<'ui>,
    ctx: &mut UiCtxLog,
    pool: &mut LogPoolPtr,
    num_frames: i64,
) -> Option<LogViewCommand> {
    let mut opened = ctx.opened;
    let mut force_loc = None;

    Window::new(&im_str!("{}", ctx.name))
        .size([600.0, 300.0], Condition::FirstUseEver)
        .opened(&mut opened)
        .build(ui, || {
            let pool = pool.lock().unwrap();
            let mut update_filter = false;

            // Activate / deactivate follow mode
            let mut following_changed = ui.checkbox(im_str!("Follow"), &mut ctx.following);
            if ui.is_item_hovered() {
                ui.tooltip_text(im_str!("Display new loglines as they arrive"));
            }
            if is_shortcut_pressed(ui, Scancode::F as _) {
                ctx.following = !ctx.following;
                following_changed = true;
            }
            ui.same_line(0.0);

            // Select minimum visible logging level
            if render_filter_by_levels(ui, &mut ctx.view) {
                update_filter = true;
            }
            ui.same_line(0.0);

            // Filter by module
            if render_filter_by_modules(ui, &mut ctx.view, &pool) {
                update_filter = true;
            }
            ui.same_line(0.0);

            // Filter by frame
            if render_filter_by_frame(ui, &mut ctx.view, num_frames) {
                update_filter = true;
            }
            ui.same_line(0.0);

            // Full-text search in logs
            let right_section = ui.window_size()[0] - 150.0;
            ui.set_next_item_width(200.0_f32.min(right_section - ui.cursor_pos()[0] - 20.0));
            if render_filter_by_text(ui, &mut ctx.view) {
                update_filter = true;
            }
            ui.same_line(right_section);

            // Save logs to disk
            render_save(ui, &mut ctx.view, &pool);
            ui.same_line(0.0);

            // Simple count of the displayed log-lines
            let numlines = ctx.filter_count.unwrap_or(0);
            if numlines >= 1000000 {
                ui.text(im_str!("Logs: {:.1}M", numlines as f32 / 1000000.0));
            } else if numlines >= 10000 {
                ui.text(im_str!("Logs: {}K", numlines / 1000));
            } else if numlines >= 1000 {
                ui.text(im_str!("Logs: {:.1}K", numlines as f32 / 1000.0));
            } else {
                ui.text(im_str!("Logs: {}", numlines));
            }

            ui.separator();

            // Function to clear the internal line cache (after a filter update)
            let clear_line_cache = |ctx: &mut UiCtxLog| {
                ctx.filter_count = None;
                ctx.cached_lines.clear();
                if !ctx.following {
                    // TODO: here, we could try to keep the same position after
                    // filter change.
                    ui.set_scroll_y(0.0);
                }
            };

            // Childwindow with scrolling bars that contains the loglines grid
            ChildWindow::new(&im_str!("##scrolling"))
                .size([0.0, 0.0])
                .always_horizontal_scrollbar(true)
                .always_vertical_scrollbar(true)
                .content_size([3000.0, 0.0])
                .build(ui, || {
                    // If the user scrolled up, automatically disable following. Just don't do
                    // it on the very first frame in which following was turned on.
                    if !following_changed && ctx.following && ui.scroll_y() != ui.scroll_max_y() {
                        ctx.following = false;
                    }

                    // If the filter was just updated, we need to inform the logpool
                    // and clear our internal caches.
                    if update_filter {
                        clear_line_cache(ctx);
                    }

                    // Refresh filter count every second or so, unless
                    // it's very fast to do so.
                    if ctx.view.is_fast_count() || ctx.last_filter_count.elapsed().as_secs() >= 1 {
                        ctx.filter_count = None;
                    }
                    let num_lines = match ctx.filter_count {
                        None => {
                            let nl = ctx.view.count();
                            ctx.filter_count = Some(nl);
                            ctx.last_filter_count = Instant::now();
                            nl
                        }
                        Some(nl) => nl,
                    };

                    let mut line_popup = false;
                    let mut kv_popup = false;

                    // Create a column grid. Imgui doesn't have helpers for configuring
                    // it only the first time (and then let the user resize it), so we
                    // need to handle that manually.
                    ui.columns(5, im_str!("##col"), true);
                    if !ctx.configured_columns {
                        ui.set_column_width(0, 60.0); // Frame number
                        ui.set_column_width(1, 46.0); // Log Level
                        ui.set_column_width(2, 140.0); // Module
                        ui.set_column_width(3, 260.0); // Message
                        ctx.configured_columns = true;
                    }

                    // Use a clipper to go through the grid and only draw visibile lines.
                    ImGuiListClipper::new(num_lines)
                        .items_height(ui.text_line_height())
                        .build(|start, end| {
                            let lines_iter = if ctx.following {
                                // If we're in following mode, just cheat and always load from the
                                // tail. We need to store the lines for borrowing rules, but let's always
                                // invalidate it right away as we don't need to cache it: the logpool
                                // query is very fast.
                                ctx.cached_lines = ctx.view.last((end - start) as usize);
                                ctx.cached_start_line = usize::max_value();
                                ctx.cached_lines.iter()
                            } else {
                                // Normal mode, we use a local cache to avoid querying the logpool too much.
                                // See if the lines that we need to draw are already in the cache
                                if ctx.cached_start_line > start as usize
                                    || ctx.cached_start_line + ctx.cached_lines.len() < end as usize
                                {
                                    // println!("updating cache: numlines:{}, cache_start:{}, start:{}, cache_start+len:{}, end:{}", numlines, ctx.cached_start_line, start, ctx.cached_start_line+ctx.cached_lines.len(), end);
                                    // Extract some additional lines from the pool, so that we don't have
                                    // to query it anytime we change position a little bit.
                                    let (start, end) = (
                                        (start as usize).saturating_sub(64),
                                        (end as usize).saturating_add(64),
                                    );
                                    ctx.cached_lines = ctx.view.get(start as u32, end as u32);
                                    ctx.cached_start_line = start;
                                    // println!("cache updated: cache_start:{}, cache_len:{}", ctx.cached_start_line, ctx.cached_lines.len());
                                }

                                let first = start as usize - ctx.cached_start_line;
                                let last =
                                    first + ((end - start) as usize).min(ctx.cached_lines.len());

                                // println!("start:{}, end:{}, cache_start:{}, cache_len:{}, first:{}, last:{}", start, end, ctx.cached_start_line, ctx.cached_lines.len(), first, last);
                                ctx.cached_lines[first..last].iter()
                            };

                            // Now go through the loglines and draw them.
                            for v in lines_iter {
                                let mouse_y = ui.io().mouse_pos[1] - ui.cursor_screen_pos()[1];
                                let hovered = mouse_y >= 0.0 && mouse_y < ui.text_line_height();

                                if !ctx.following {
                                    // Use a selectable to show a colored background over the whole
                                    // row (this seems the only available option right now).
                                    // Don't bother trying to use it for clicking, as we're not
                                    // actually using its logic.
                                    let ct = ui.push_style_color(
                                        StyleColor::HeaderHovered,
                                        [0.2, 0.2, 0.2, 1.0],
                                    );
                                    Selectable::new(im_str!("##sel"))
                                        .span_all_columns(true)
                                        .build(ui);
                                    ui.set_item_allow_overlap();
                                    ct.pop(ui);
                                    ui.same_line(-1.0);
                                }

                                if v.location.is_some() {
                                    ui.text_disabled("*");
                                    if !ctx.following && ui.is_item_hovered() {
                                        ui.tooltip_text(im_str!(
                                            "Generated at: {}",
                                            v.location.as_ref().unwrap()
                                        ));
                                    }
                                } else {
                                    ui.text_disabled(" ");
                                }
                                ui.same_line(0.0);
                                ui.text_colored(COLOR_FRAME, im_str!("[{}]", v.frame));
                                ui.next_column();
                                ui.text_colored(
                                    LOG_LEVEL_COLOR[v.level as usize],
                                    im_str!("{}", LOG_LEVEL_SHORT_NAMES[v.level as usize]),
                                );
                                ui.next_column();
                                ui.text_colored(COLOR_MODULE, &pool.modules[v.module as usize]);
                                ui.next_column();
                                ui.text(im_str!("{:80}", v.msg));
                                ui.next_column();
                                ui.text(im_str!("{}", v.kv.replace("\t", " ")));
                                ui.next_column();

                                if !ctx.following
                                    && hovered
                                    && ui.is_mouse_clicked(MouseButton::Right)
                                {
                                    ctx.selected = v.clone();
                                    line_popup = true;
                                }
                            }
                        });

                    if !ctx.following {
                        if line_popup {
                            ui.open_popup(im_str!("##line_popup"))
                        }
                        ui.popup(im_str!("##line_popup"), || {
                            if MenuItem::new(&im_str!("Show only frame {}", ctx.selected.frame))
                                .build(ui)
                            {
                                ctx.view.set_filter_min_frame(Some(ctx.selected.frame));
                                ctx.view.set_filter_max_frame(Some(ctx.selected.frame));
                                clear_line_cache(ctx);
                            }
                            if MenuItem::new(&im_str!(
                                "Show only module {}",
                                pool.modules[ctx.selected.module as usize]
                            ))
                            .build(ui)
                            {
                                ctx.view.set_filter_modules(Some(vec![ctx.selected.module]));
                                clear_line_cache(ctx);
                            }
                            if MenuItem::new(&im_str!(
                                "Show only messages \"{}\"",
                                if ctx.selected.msg.len() > 10 {
                                    ctx.selected.msg[0..10].to_owned() + "[..]"
                                } else {
                                    ctx.selected.msg.clone()
                                }
                            ))
                            .build(ui)
                            {
                                ctx.view.set_filter_text(Some(&ctx.selected.msg));
                                clear_line_cache(ctx);
                            }
                            ui.separator();
                            if let Some(loc) = ctx.selected.location() {
                                if MenuItem::new(&im_str!(
                                    "Go to {}..",
                                    ctx.selected.location.as_ref().unwrap()
                                ))
                                .build(ui)
                                {
                                    force_loc = Some(loc);
                                }
                            }
                            if MenuItem::new(im_str!("Expand key/values..")).build(ui) {
                                kv_popup = true;
                            }
                        });
                        if kv_popup {
                            ui.open_popup(im_str!("##kv_popup"));
                        }
                        ui.popup(im_str!("##kv_popup"), || {
                            ChildWindow::new(im_str!("##child"))
                                .size([300.0, 200.0])
                                .horizontal_scrollbar(true)
                                .build(&ui, || {
                                    ui.columns(2, im_str!("##col"), true);
                                    if kv_popup {
                                        // only when jsut opened, set column widths
                                        ui.set_column_width(0, 50.0);
                                        ui.set_column_width(1, 250.0);
                                    }
                                    let style = ui.clone_style();
                                    let glyph_width =
                                        ui.calc_text_size(im_str!("F"), false, 0.0)[0];
                                    let vchars = (ui.column_width(1) - style.item_spacing[0] * 2.0)
                                        / glyph_width;
                                    ui.text(im_str!("Key"));
                                    ui.next_column();
                                    ui.text(im_str!("Value"));
                                    ui.next_column();
                                    ui.separator();
                                    for (n, kv) in ctx.selected.kv.split('\t').enumerate() {
                                        let mut kv = kv.splitn(2, "=");
                                        let k = kv.next().unwrap();
                                        let v = kv.next().unwrap_or("");

                                        ui.text(im_str!("{}", k));
                                        ui.next_column();

                                        let mut buf =
                                            im_str!("{}", textwrap::fill(v, vchars as usize))
                                                .to_owned();
                                        if v.len() > 32 {
                                            ui.input_text_multiline(
                                                &im_str!("##v{}", n),
                                                &mut buf,
                                                [240.0, 0.0],
                                            )
                                            .auto_select_all(true)
                                            .read_only(true)
                                            .build();
                                        } else {
                                            ui.set_next_item_width(240.0);
                                            ui.input_text(&im_str!("##v{}", n), &mut buf)
                                                .auto_select_all(true)
                                                .read_only(true)
                                                .build();
                                        }
                                        ui.next_column();
                                    }
                                })
                        });
                    }
                    if ctx.following {
                        ui.set_scroll_here_y();
                    }
                });
        });

    ctx.opened = opened;

    if let Some((cpu, pc)) = force_loc {
        return Some(LogViewCommand::ShowPc(cpu, pc));
    }
    return None;
}
