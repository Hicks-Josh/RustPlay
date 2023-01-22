use egui::{vec2, Align2, Color32, Id, Ui, Vec2, Window};
use egui_dock::{DockArea, Node, NodeIndex, Style, TabAddAlign, TabIndex};
use serde::{Deserialize, Serialize};

use crate::config::{Command, Config, GitHub, MenuCommand, TabCommand};
use crate::utils::data::Data;

use super::code_editor::CodeEditor;
use super::titlebar::TITLEBAR_HEIGHT;

pub type Tree = egui_dock::Tree<Tab>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tab {
    name: String,
    editor: CodeEditor,
    id: Id,
    scroll_offset: Option<Vec2>,
}

pub trait TreeTabs
where
    Self: Sized,
{
    fn init() -> Self;
}

// Initialize the initial tabs / tab data
impl TreeTabs for Tree {
    fn init() -> Self {
        let tab = Tab {
            name: "Scratch 1".to_string(),
            editor: CodeEditor::default(),
            id: Id::new("Scratch 1"),
            scroll_offset: None,
        };

        let mut tree = Tree::new(vec![tab]);
        tree.set_focused_node(NodeIndex::root());
        tree
    }
}

pub struct Dock;

impl Dock {
    pub fn show(ctx: &egui::Context, config: &mut Config, ui: &mut Ui) {
        let tree = &mut config.dock.tree;

        let mut style = Style::from_egui(ctx.style().as_ref());

        // important, otherwise it'll draw over the original titlebar
        style.tab_bar_background_color = Color32::TRANSPARENT;
        style.tab_bar_height = TITLEBAR_HEIGHT as f32 / 2.0;
        style.tabs_are_draggable = true;
        style.tab_include_scrollarea = false;
        style.show_add_buttons = true;
        style.add_tab_align = TabAddAlign::Left;
        style.show_context_menu = true;

        let tab_data = TabData::new();

        let active_id = if let Some((_, tab)) = tree.find_active_focused() {
            tab.id
        } else {
            Id::new("")
        };

        let mut tab_viewer = TabViewer::new(ctx, &tab_data, active_id);

        DockArea::new(tree)
            .style(style.clone())
            .show_inside(ui, &mut tab_viewer);

        // add data to command vec
        config
            .dock
            .commands
            .extend_from_slice(tab_data.borrow().as_slice());
    }
}

type TabData = Data<Command>;

struct TabViewer<'a> {
    _ctx: &'a egui::Context,
    data: &'a TabData,
    focused_tab: Id,
}

impl<'a> TabViewer<'a> {
    fn new(ctx: &'a egui::Context, data: &'a TabData, focused_tab: Id) -> Self {
        Self {
            _ctx: ctx,
            data,
            focused_tab,
        }
    }
}

impl egui_dock::TabViewer for TabViewer<'_> {
    type Tab = Tab;

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        // multiple tabs may be open on the screen, so we need to know if one is focused or not so we don't steal focus
        tab.scroll_offset = Some(tab.editor.show(
            tab.id,
            ui,
            tab.scroll_offset.unwrap_or_default(),
            tab.id == self.focused_tab,
        ));
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        (&*tab.name).into()
    }

    fn on_add(&mut self, node: NodeIndex) {
        let mut data = self.data.borrow_mut();
        data.push(Command::TabCommand(TabCommand::Add(node)));
    }

    fn context_menu(
        &mut self,
        ui: &mut Ui,
        _tab: &mut Self::Tab,
        tabindex: TabIndex,
        nodeindex: NodeIndex,
    ) {
        let mut data = self.data.borrow_mut();

        let rename_btn = ui.button("Rename".to_string()).clicked();
        let save_btn = ui.button("Save...".to_string()).clicked();
        let share_btn = ui.button("Share to Playground".to_string()).clicked();

        let mut command = None;

        if rename_btn {
            command = Some(MenuCommand::Rename((nodeindex, tabindex)));
        }

        if save_btn || share_btn {
            let data = (nodeindex, tabindex);
            command = Some(if save_btn {
                MenuCommand::Save(data)
            } else {
                MenuCommand::Share(data)
            });
        }

        if let Some(command) = command {
            data.push(Command::MenuCommand(command));
            ui.close_menu();
        }
    }

    fn on_close(&mut self, _tab: &mut Self::Tab) -> bool {
        let mut data = self.data.borrow_mut();
        data.push(Command::TabCommand(TabCommand::Close));

        true
    }
}

#[derive(Debug)]
pub struct TabEvents;

impl TabEvents {
    pub fn show(ctx: &egui::Context, config: &mut Config) {
        // Functions which return false remove their item from the vec.
        config.dock.commands.retain(|i| match i {
            Command::MenuCommand(command) => match command {
                MenuCommand::Rename(v) => Self::show_rename_window(ctx, *v, &mut config.dock.tree),
                MenuCommand::Save(_) => todo!(),
                MenuCommand::Share(v) => {
                    Self::share_scratch(*v, &mut config.dock.tree, &config.github)
                }
            },

            Command::TabCommand(command) => match command {
                TabCommand::Add(v) => {
                    let name = format!("Scratch {}", config.dock.counter);

                    let node_tabs = &config.dock.tree[*v];

                    let tab = Tab {
                        // unique name based on current nodeindex + tabindex
                        id: Id::new(format!("{name}-{}-{}", v.0, node_tabs.tabs_count() + 1)),
                        name,
                        editor: CodeEditor::default(),
                        scroll_offset: None,
                    };

                    config.dock.tree.set_focused_node(*v);
                    config.dock.tree.push_to_focused_leaf(tab);

                    config.dock.counter += 1;

                    false
                }

                TabCommand::Close => {
                    if config.dock.tree.num_tabs() == 0 {
                        let tab = Tab {
                            name: "Scratch 1".to_string(),
                            editor: CodeEditor::default(),
                            id: Id::new("Scratch 1"),
                            scroll_offset: None,
                        };

                        config.dock.tree.set_focused_node(NodeIndex(0));
                        config.dock.tree.push_to_focused_leaf(tab);

                        config.dock.counter = 2;
                    }

                    false
                }
            },
        });
    }

    fn show_rename_window(
        ctx: &egui::Context,
        (nodeindex, tabindex): (NodeIndex, TabIndex),
        tree: &mut Tree,
    ) -> bool {
        // Get the tabs for the specified nodeindex
        let Node::Leaf {
            tabs,
            ..
        } = &mut tree[nodeindex] else {
            unreachable!();
        };

        // And get the tab by index
        let tab = &mut tabs[tabindex.0];

        Window::new(&format!("Rename {}", tab.name))
            .title_bar(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .auto_sized()
            .show(ctx, |ui| {
                if ui.button("Done").clicked() {
                    tab.name = "nice".to_string();
                    return false;
                }

                true
            })
            .unwrap()
            .inner
            .unwrap()
    }

    fn share_scratch(
        (nodeindex, tabindex): (NodeIndex, TabIndex),
        tree: &mut Tree,
        github: &GitHub,
    ) -> bool {
        println!("shared scratch token: {}", github.access_token);

        false
    }
}
