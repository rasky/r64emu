use super::uisupport::{ctext, ImGuiListClipper};
use super::{UiCtx, UiCtxLog};
use crate::log::{LogPool, LogPoolPtr, LogView};
use sdl2::keyboard::Scancode;

use imgui::*;
use slog::LOG_LEVEL_SHORT_NAMES;

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

fn render_filter_by_levels<'a, 'ui>(ui: &'a Ui<'ui>, view: &mut LogView) -> bool {
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
            Some(slog::Level::from_usize(max_level).unwrap())
        });
        update_filter = true;
    }
    if ui.is_item_hovered() {
        ui.tooltip_text(im_str!("Minimum displayed level"));
    }
    update_filter
}

fn render_filter_by_modules<'a, 'ui>(ui: &'a Ui<'ui>, view: &mut LogView, pool: &LogPool) -> bool {
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

pub fn render_filter_by_text<'a, 'ui>(ui: &'a Ui<'ui>, view: &mut LogView) -> bool {
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

pub(crate) fn render_logview<'a, 'ui>(ui: &'a Ui<'ui>, ctx: &mut UiCtxLog, pool: &mut LogPoolPtr) {
    let mut opened = ctx.opened;
    Window::new(&im_str!("Logs"))
        .size([500.0, 300.0], Condition::FirstUseEver)
        .opened(&mut opened)
        .build(ui, || {
            let pool = pool.lock().unwrap();
            let mut update_filter = false;

            // Activate / deactivate follow mode
            let mut following_changed = ui.checkbox(im_str!("Follow"), &mut ctx.following);
            if ui.is_item_hovered() {
                ui.tooltip_text(im_str!("Display new loglines as they arrive"));
            }
            if !ui.io().want_text_input && ui.is_key_pressed(Scancode::F as _) {
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

            // Full-text search in logs
            let right_section = ui.window_size()[0] - 80.0;
            ui.set_next_item_width(200.0_f32.min(right_section - ui.cursor_pos()[0] - 20.0));
            if render_filter_by_text(ui, &mut ctx.view) {
                update_filter = true;
            }
            ui.same_line(right_section);

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

            // Childwindow with scrolling bars that contains the loglines grid
            ChildWindow::new(&im_str!("##scrolling"))
                .size([0.0, 0.0])
                .always_horizontal_scrollbar(true)
                .always_vertical_scrollbar(true)
                .build(ui, || {
                    // If the user scrolled up, automatically disable following. Just don't do
                    // it on the very first frame in which following was turned on.
                    if !following_changed && ctx.following && ui.scroll_y() != ui.scroll_max_y() {
                        ctx.following = false;
                    }

                    // If the filter was just updated, we need to inform the logpool
                    // and clear our internal caches.
                    if update_filter {
                        ctx.filter_count = None;
                        ctx.cached_lines.clear();
                        if !ctx.following {
                            // TODO: here, we could try to keep the same position after
                            // filter change.
                            ui.set_scroll_y(0.0);
                        }
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

                    let mut kv_popup = false;

                    // Create a column grid. Imgui doesn't have helpers for configuring
                    // it only the first time (and then let the user resize it), so we
                    // need to handle that manually.
                    ui.columns(5, im_str!("##col"), true);
                    if !ctx.configured_columns {
                        ui.set_column_width(0, 50.0); // Frame number
                        ui.set_column_width(1, 50.0); // Log Level
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
                                ctx.cached_lines = ctx.view.last((end - start + 1) as usize);
                                ctx.cached_start_line = usize::max_value();
                                ctx.cached_lines.iter()
                            } else {
                                // Normal mode, we use a local cache to avoid querying the logpool too much.
                                // See if the lines that we need to draw are already in the cache
                                if ctx.cached_start_line > start as usize
                                    || ctx.cached_start_line + ctx.cached_lines.len() < end as usize
                                {
                                    // Extract some additional lines from the pool, so that we don't have
                                    // to query it anytime we change position a little bit.
                                    let (start, end) = (
                                        (start as usize).saturating_sub(64),
                                        (end as usize).saturating_add(64),
                                    );
                                    ctx.cached_lines = ctx.view.get(start as u32, end as u32);
                                    ctx.cached_start_line = start;
                                }

                                let first = start as usize - ctx.cached_start_line;
                                let last = (first + (end - start + 1) as usize)
                                    .min(ctx.cached_lines.len());

                                ctx.cached_lines[first..last].iter()
                            };

                            // Now go through the loglines and draw them.
                            for v in lines_iter {
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
                                if !ctx.following && ui.is_item_clicked(MouseButton::Left) {
                                    ctx.selected = v.kv.clone();
                                    kv_popup = true;
                                }
                                ui.next_column();
                            }
                        });

                    if !ctx.following {
                        if kv_popup {
                            ui.open_popup(im_str!("##kv"));
                        }
                        ui.popup(im_str!("##kv"), || {
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
                                    ui.text(im_str!("Key"));
                                    ui.next_column();
                                    ui.text(im_str!("Value"));
                                    ui.next_column();
                                    ui.separator();
                                    for (n, kv) in ctx.selected.split('\t').enumerate() {
                                        let mut kv = kv.splitn(2, "=");
                                        let k = kv.next().unwrap();
                                        let v = kv.next().unwrap_or("");

                                        ui.text(im_str!("{}", k));
                                        ui.next_column();

                                        let mut buf = im_str!("{}", v).to_owned();
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
}
