use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use egui::mutex::Mutex;
use egui::panel::PanelState;
use egui::text::LayoutJob;
use egui::{pos2, vec2, Color32, CursorIcon, FontId, Id, Rect, Sense, Stroke, TextBuffer, Vec2};
use once_cell::sync::OnceCell;

use crate::config::{AnsiColors, Config};
use crate::utils::ansi_parser::{self, Color};

use super::titlebar::TITLEBAR_HEIGHT;

// A read only string for multiline textedit
struct ReadOnlyString<'a> {
    content: &'a str,
}

impl<'a> TextBuffer for ReadOnlyString<'a> {
    fn is_mutable(&self) -> bool {
        false
    }

    fn as_str(&self) -> &str {
        self.content
    }

    fn insert_text(&mut self, _: &str, _: usize) -> usize {
        0
    }

    fn delete_char_range(&mut self, _: std::ops::Range<usize>) {}

    fn clear(&mut self) {}

    fn replace(&mut self, _: &str) {}
}

impl<'a> ReadOnlyString<'a> {
    fn new(content: &'a str) -> Self {
        Self { content }
    }
}

// Memoized ansi color parsing
pub fn parse_ansi(
    ctx: &egui::Context,
    ansi_colors: AnsiColors,
    unparsed_text: &str,
    text: &str,
) -> LayoutJob {
    impl egui::util::cache::ComputerMut<(u64, Color32, AnsiColors, &str, &str), LayoutJob>
        for AnsiColorParser
    {
        fn compute(
            &mut self,
            (_, default_color, ansi_colors, unparsed_text, text): (
                u64,
                Color32,
                AnsiColors,
                &str,
                &str,
            ),
        ) -> LayoutJob {
            self.parse(default_color, ansi_colors, unparsed_text, text)
        }
    }

    type ColorCache = egui::util::cache::FrameCache<LayoutJob, AnsiColorParser>;

    let mut s = DefaultHasher::new();
    unparsed_text.hash(&mut s);
    let hash = s.finish();

    let default_color = { ctx.style().visuals.text_color() };

    let mut memory = ctx.memory();
    let color_cache = memory.caches.cache::<ColorCache>();
    color_cache.get((hash, default_color, ansi_colors, unparsed_text, text))
}

struct AnsiColorParser;

impl Default for AnsiColorParser {
    fn default() -> Self {
        Self
    }
}

impl AnsiColorParser {
    fn parse(
        &self,
        default_color: Color32,
        colors: AnsiColors,
        unparsed_text: &str,
        text: &str,
    ) -> LayoutJob {
        let ansi_to_color32 = |color| match color {
            Color::Black => colors.black.to_color32(),
            Color::Red => colors.red.to_color32(),
            Color::Green => colors.green.to_color32(),
            Color::Yellow => colors.yellow.to_color32(),
            Color::Blue => colors.blue.to_color32(),
            Color::Magenta => colors.magenta.to_color32(),
            Color::Cyan => colors.cyan.to_color32(),
            Color::White => colors.white.to_color32(),
            Color::BrightBlack => colors.bright_black.to_color32(),
            Color::BrightRed => colors.bright_red.to_color32(),
            Color::BrightGreen => colors.bright_green.to_color32(),
            Color::BrightYellow => colors.bright_yellow.to_color32(),
            Color::BrightBlue => colors.bright_blue.to_color32(),
            Color::BrightMagenta => colors.bright_magenta.to_color32(),
            Color::BrightCyan => colors.bright_cyan.to_color32(),
            Color::BrightWhite => colors.bright_white.to_color32(),
            Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        };

        use egui::text::{LayoutSection, TextFormat};

        let parsed = ansi_parser::parse(unparsed_text);

        let mut job = LayoutJob {
            text: text.into(),
            ..Default::default()
        };

        for chunk in parsed.properties {
            let text_color = chunk.fg.map(ansi_to_color32).unwrap_or(default_color);
            let background_color = chunk.bg.map(ansi_to_color32).unwrap_or(Color32::default());

            let italics = chunk.style.italic;
            let underline = chunk.style.underline;

            let underline = if underline {
                Stroke::new(1.0, text_color)
            } else {
                Stroke::NONE
            };

            let strikethrough = if chunk.style.strikethrough {
                Stroke::new(1.0, text_color)
            } else {
                Stroke::NONE
            };

            job.sections.push(LayoutSection {
                leading_space: 0.0,
                byte_range: chunk.start..chunk.end,
                format: TextFormat {
                    font_id: FontId::monospace(12.0),
                    color: text_color,
                    italics,
                    underline,
                    background: background_color,
                    strikethrough,
                    ..Default::default()
                },
            });
        }

        job
    }
}

pub struct Terminal;

impl Terminal {
    pub fn show(ctx: &egui::Context, config: &mut Config) {
        let id = Id::new("terminal");

        if config.terminal.opened_from_close {
            // we need to reset the panel state position to be where the mouse pointer is to make it seamless
            // on open, so it doesn't flash when opening by opening big then resetting to where the mouse is
            let coords = ctx.pointer_latest_pos().unwrap_or_default();
            let window_rect = ctx.available_rect();
            let rect = Rect::from_two_pos(
                pos2(0.0, coords.y),
                pos2(window_rect.right(), window_rect.bottom()),
            );

            ctx.data().insert_persisted(id, PanelState { rect });
        }

        egui::TopBottomPanel::bottom(id)
            .resizable(true)
            .default_height(0.0)
            .min_height(0.0)
            .max_height(ctx.available_rect().height() - (TITLEBAR_HEIGHT as f32 / 2.0))
            .show_separator_line(false)
            .show(ctx, |ui| {
                //
                // Panel handling code
                //

                let mut close_rect = ctx.available_rect();

                let close_threshold = if config.terminal.opened_from_close_dragging {
                    16.0
                } else {
                    20.0
                };

                close_rect.set_top(close_rect.bottom() - close_threshold);

                let pointer_pos = ctx.pointer_latest_pos().unwrap_or_default();

                let window_close_bottom = ctx.available_rect().bottom() - close_threshold;

                let resize_id = id.with("__resize");

                // when mouse is outside of window, as long as we were dragging, pointer_pos is still Some()
                // we can utilize this to allow resizing AS LONG AS mouse isn't below the window in screen coords
                if (close_rect.contains(pointer_pos) || pointer_pos.y >= window_close_bottom)
                    && ctx.memory().is_being_dragged(resize_id)
                {
                    config.terminal.open = false;
                    config.terminal.closed_from_open = true;
                }

                if config.terminal.opened_from_close {
                    let mut memory = ui.memory();
                    memory.set_dragged_id(resize_id);

                    config.terminal.opened_from_close = false;
                }

                if config.terminal.opened_from_close_dragging
                    && !ui.memory().is_being_dragged(resize_id)
                {
                    config.terminal.opened_from_close_dragging = false;
                }

                //
                // Scrollbar and panel contents
                //

                let mut frame_rect = ui.max_rect();
                frame_rect.set_left(frame_rect.left() + 2.0);
                frame_rect.set_right(frame_rect.right() - 2.0);
                frame_rect.set_bottom(frame_rect.bottom() - 10.0);
                frame_rect.set_top(frame_rect.top() + 10.0);

                let active_tab = config.terminal.active_tab.unwrap();
                let offset = *config
                    .terminal
                    .scroll_offset
                    .get_mut(&active_tab)
                    .unwrap_or(&mut Vec2::default());

                let terminal_output = config.terminal.content.entry(active_tab).or_default();
                let terminal_output_stdout = terminal_output.0.lock().unwrap();
                let terminal_output_stderr = terminal_output.1.lock().unwrap();

                //
                // Parsing and caching
                //
                static CACHE_STDOUT: OnceCell<Arc<Mutex<HashMap<Id, (u64, String)>>>> =
                    OnceCell::new();
                static CACHE_STDERR: OnceCell<Arc<Mutex<HashMap<Id, (u64, String)>>>> =
                    OnceCell::new();
                let mut cache_stdout = CACHE_STDOUT
                    .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
                    .lock();
                let mut cache_stderr = CACHE_STDERR
                    .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
                    .lock();

                let restrip = |text: &str| {
                    let stripped =
                        String::from_utf8(strip_ansi_escapes::strip(text).unwrap()).unwrap();
                    let mut s = DefaultHasher::new();
                    text.hash(&mut s);
                    let hash = s.finish();

                    (hash, stripped)
                };

                let (hash_stdout, plain_stdout) = cache_stdout
                    .entry(active_tab)
                    .or_insert_with(|| restrip(&terminal_output_stdout));
                let (hash_stderr, plain_stderr) = cache_stderr
                    .entry(active_tab)
                    .or_insert_with(|| restrip(&terminal_output_stderr));

                let mut s = DefaultHasher::new();
                let mut s2 = DefaultHasher::new();
                terminal_output_stdout.hash(&mut s);
                terminal_output_stderr.hash(&mut s2);
                let new_hash_stdout = s.finish();
                let new_hash_stderr = s2.finish();

                // if hash isn't the same, then recalculate and re-save it
                if *hash_stdout != new_hash_stdout {
                    (*hash_stdout, *plain_stdout) = restrip(&terminal_output_stdout);
                }
                if *hash_stderr != new_hash_stderr {
                    (*hash_stderr, *plain_stderr) = restrip(&terminal_output_stderr);
                }

                let mut read_only_term_stdout = ReadOnlyString::new(&plain_stdout);
                let mut read_only_term_stderr = ReadOnlyString::new(&plain_stderr);

                let ansi_colors = config.theme.ansi_colors;

                let mut layouter = |ui: &egui::Ui, text: &str, wrap_width: f32| {
                    let mut layout_job =
                        parse_ansi(ui.ctx(), ansi_colors, &terminal_output_stdout, text);
                    layout_job.wrap.max_width = wrap_width;
                    ui.fonts().layout_job(layout_job)
                };
                let mut layouter2 = |ui: &egui::Ui, text: &str, wrap_width: f32| {
                    let mut layout_job =
                        parse_ansi(ui.ctx(), ansi_colors, &terminal_output_stderr, text);
                    layout_job.wrap.max_width = wrap_width;
                    ui.fonts().layout_job(layout_job)
                };

                let text_widget_stdout = egui::TextEdit::multiline(&mut read_only_term_stdout)
                    .font(egui::TextStyle::Monospace) // for cursor height
                    // remove the frame and draw our own
                    .frame(false)
                    .desired_width(f32::INFINITY)
                    .layouter(&mut layouter)
                    .id(id.with("term_output_stdout"))
                    .interactive(true);

                let text_widget_stderr = egui::TextEdit::multiline(&mut read_only_term_stderr)
                    .font(egui::TextStyle::Monospace) // for cursor height
                    // remove the frame and draw our own
                    .frame(false)
                    .desired_width(f32::INFINITY)
                    .layouter(&mut layouter2)
                    .id(id.with("term_output_stderr"))
                    .interactive(true);

                let scrollarea = egui::ScrollArea::vertical()
                    .max_height(f32::INFINITY)
                    .auto_shrink([false, false])
                    .scroll_offset(offset)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.heading("Standard Error");
                                ui.add(text_widget_stderr);
                            });
                        });

                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.heading("Standard Output");
                                ui.add(text_widget_stdout);
                            });
                        });
                    });

                config
                    .terminal
                    .scroll_offset
                    .insert(active_tab, scrollarea.state.offset);
            });
    }

    pub fn show_closed_handle(ctx: &egui::Context, config: &mut Config) {
        let id = Id::new("terminal-closed");

        egui::TopBottomPanel::bottom(id)
            .resizable(false)
            .default_height(13.0)
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.vertical_centered(|ui| {
                        let center_id = id.with("center_line");

                        let sense = Sense::click_and_drag();
                        let hover_sense = Sense::hover();

                        let (alloc_id, center_line) = ui.allocate_space(vec2(75.0, 2.0));
                        let response = ui.interact(center_line, alloc_id, sense);
                        let h_response =
                            ui.interact(center_line, center_id.with("hover"), hover_sense);

                        if config.terminal.closed_from_open {
                            ui.memory().set_dragged_id(alloc_id);
                            config.terminal.closed_from_open = false;
                        }

                        let is_dragging = response.dragged();

                        if is_dragging || h_response.hovered() {
                            ui.output().cursor_icon = CursorIcon::ResizeVertical;
                        }

                        // we need to subtract the closing threshold from the bottom
                        let window_bottom = ctx.available_rect().bottom() - 17.0;

                        if response.drag_delta().y <= -0.5
                            && ctx.pointer_latest_pos().unwrap_or_default().y <= window_bottom
                        {
                            config.terminal.open = true;
                            config.terminal.opened_from_close = true;
                            config.terminal.opened_from_close_dragging = true;
                        }

                        let stroke = if is_dragging {
                            ui.style().visuals.widgets.active.bg_stroke
                        } else if h_response.hovered() {
                            ui.style().visuals.widgets.hovered.bg_stroke
                        } else {
                            ui.style().visuals.widgets.noninteractive.bg_stroke
                        };

                        ui.painter().rect_filled(center_line, 2.0, stroke.color);
                    });
                });
            });
    }
}
