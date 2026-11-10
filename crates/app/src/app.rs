use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use eframe::egui::{self, Color32, RichText};
use mathseea_graph_engine::highlight;
use mathseea_graph_view::{GraphView, NebulaView, ViewEvent, ViewOptions};
use mathseea_model::{GraphError, Highlight, HighlightMode, NodeId, Selection};

use crate::cli::Source;
use crate::document::{self, LoadedGraph};
use crate::theme;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    Bundled,
    Graph,
    Mathlib,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ViewMode {
    #[default]
    Topology,
    Nebula,
}

enum Completed {
    Loaded(Arc<LoadedGraph>),
    Exported(PathBuf),
}

pub struct Explorer {
    document: Option<Arc<LoadedGraph>>,
    view: GraphView,
    nebula: Option<NebulaView>,
    view_mode: ViewMode,
    navigation_open: bool,
    inspector_open: bool,
    data_open: bool,
    export_open: bool,
    topic_filter: String,
    options: ViewOptions,
    selection: Selection,
    mode: HighlightMode,
    highlighted: Option<Highlight>,
    highlighted_count: usize,
    source_kind: SourceKind,
    source_path: String,
    export_path: String,
    search: String,
    search_results: Vec<NodeId>,
    pending: Option<mpsc::Receiver<Result<Completed, GraphError>>>,
    task_label: &'static str,
    error: Option<String>,
    notice: Option<String>,
}

impl Explorer {
    pub fn new(context: &eframe::CreationContext<'_>, source: Option<Source>) -> Self {
        theme::apply(&context.egui_ctx);
        install_cjk_font(&context.egui_ctx);
        let mut app = Self::empty(context.wgpu_render_state.as_ref().map(NebulaView::new));
        if let Some(source) = source {
            app.source_path = source.path().to_string_lossy().into_owned();
            app.source_kind = match &source {
                Source::Bundled => SourceKind::Bundled,
                Source::Graph(_) => SourceKind::Graph,
                Source::Mathlib(_) => SourceKind::Mathlib,
            };
            app.start_load(source, &context.egui_ctx);
        }
        app
    }

    fn empty(nebula: Option<NebulaView>) -> Self {
        Self {
            document: None,
            view: GraphView::default(),
            nebula,
            view_mode: ViewMode::Topology,
            navigation_open: true,
            inspector_open: true,
            data_open: false,
            export_open: false,
            topic_filter: String::new(),
            options: ViewOptions::default(),
            selection: Selection::None,
            mode: HighlightMode::Neighbors,
            highlighted: None,
            highlighted_count: 0,
            source_kind: SourceKind::Graph,
            source_path: String::new(),
            export_path: String::new(),
            search: String::new(),
            search_results: Vec::new(),
            pending: None,
            task_label: "",
            error: None,
            notice: None,
        }
    }

    fn start_task(
        &mut self,
        label: &'static str,
        context: &egui::Context,
        work: impl FnOnce() -> Result<Completed, GraphError> + Send + 'static,
    ) {
        if self.pending.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let context = context.clone();
        match std::thread::Builder::new()
            .name("mathseea-document".into())
            .spawn(move || {
                let _ = sender.send(work());
                context.request_repaint();
            }) {
            Ok(_) => {
                self.pending = Some(receiver);
                self.task_label = label;
                self.error = None;
                self.notice = None;
            }
            Err(error) => self.error = Some(format!("无法启动后台任务: {error}")),
        }
    }

    fn start_load(&mut self, source: Source, context: &egui::Context) {
        self.start_task("正在加载和布局", context, move || {
            document::load(source).map(Arc::new).map(Completed::Loaded)
        });
    }

    fn start_export(&mut self, context: &egui::Context) {
        let Some(document) = self.document.clone() else {
            return;
        };
        if self.export_path.trim().is_empty() {
            self.error = Some("请填写导出文件路径".into());
            return;
        }
        let path = PathBuf::from(&self.export_path);
        self.start_task("正在导出", context, move || {
            document::export_new(&path, &document)?;
            Ok(Completed::Exported(path))
        });
    }

    fn poll_task(&mut self, context: &egui::Context) {
        let Some(receiver) = &self.pending else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                context.request_repaint_after(Duration::from_millis(100));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(GraphError::new("后台任务意外结束, 未收到载入或导出结果"))
            }
        };
        self.pending = None;
        match result {
            Ok(Completed::Loaded(document)) => {
                self.highlighted_count = document.graph.nodes.len();
                self.highlighted = Some(Highlight::all(&document.graph));
                self.notice = Some(format!(
                    "已加载 {} 个模块, {} 条导入关系",
                    document.graph.nodes.len(),
                    document.graph.edges.len()
                ));
                self.document = Some(document);
                self.selection = Selection::None;
                self.mode = HighlightMode::Neighbors;
                self.view.reset_camera();
                if let Some(view) = &mut self.nebula {
                    view.reset_document();
                }
                self.rebuild_search();
            }
            Ok(Completed::Exported(path)) => {
                self.notice = Some(format!("已导出到 {}", path.display()));
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn rebuild_search(&mut self) {
        self.search_results.clear();
        let query = self.search.trim().to_lowercase();
        if query.is_empty() {
            return;
        }
        if let Some(document) = &self.document {
            self.search_results.extend(
                document
                    .search_names
                    .iter()
                    .enumerate()
                    .filter_map(|(id, name)| name.contains(&query).then_some(id)),
            );
        }
    }

    fn select(&mut self, selection: Selection, focus: bool) {
        let Some(document) = &self.document else {
            return;
        };
        let mode = match &selection {
            Selection::None => HighlightMode::Neighbors,
            Selection::Node(_) if self.mode.is_topic() => HighlightMode::Neighbors,
            Selection::Topic(_) if !self.mode.is_topic() => HighlightMode::TopicMembers,
            _ => self.mode,
        };
        match highlight(&document.graph, &selection, mode) {
            Ok(mask) => {
                if focus && let Selection::Node(id) = &selection {
                    self.view.focus_node(*id);
                    if let Some(view) = &mut self.nebula {
                        view.focus_node(*id);
                    }
                }
                self.highlighted_count = mask.nodes.iter().filter(|&&on| on).count();
                self.highlighted = Some(mask);
                self.selection = selection;
                self.mode = mode;
                if self.selection != Selection::None {
                    self.inspector_open = true;
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn reset_view(&mut self) {
        self.view.reset_camera();
        if let Some(view) = &mut self.nebula {
            view.reset_camera();
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let wide = theme::workbench_size(ui.available_width()) == theme::WorkbenchSize::Wide;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("M").strong().size(22.0).color(theme::ACCENT));
            ui.label(RichText::new("MathSeea").strong().size(18.0));
            ui.separator();
            if ui
                .selectable_label(self.view_mode == ViewMode::Topology, "2D 拓扑")
                .clicked()
            {
                self.view_mode = ViewMode::Topology;
            }
            if ui
                .selectable_label(self.view_mode == ViewMode::Nebula, "3D 星云")
                .clicked()
            {
                self.view_mode = ViewMode::Nebula;
            }
            ui.separator();
            let search = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("搜索模块 / LinearAlgebra.Basis")
                    .desired_width(if wide { 280.0 } else { 180.0 }),
            );
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::K)) {
                search.request_focus();
            }
            if search.changed() {
                self.rebuild_search();
            }
            if theme::button(ui, "数据", false, self.pending.is_none()).clicked() {
                self.data_open = true;
            }
            if theme::button(
                ui,
                "导出",
                true,
                self.document.is_some() && self.pending.is_none(),
            )
            .clicked()
            {
                self.export_open = true;
            }
            if theme::button(ui, "适配", false, self.document.is_some()).clicked() {
                self.reset_view();
            }
            if !wide {
                ui.toggle_value(&mut self.navigation_open, "导航");
                ui.toggle_value(&mut self.inspector_open, "检查器");
            }
        });
        if !self.search.trim().is_empty() {
            let _ = self.search_results_ui(ui);
        }
    }

    fn search_results_ui(
        &mut self,
        ui: &mut egui::Ui,
    ) -> Option<egui::scroll_area::ScrollAreaOutput<Option<NodeId>>> {
        let document = self.document.as_ref()?;
        ui.small(format!("{} 个匹配", self.search_results.len()));
        let row_height = ui.spacing().interact_size.y;
        let output = egui::ScrollArea::vertical()
            .id_salt("command-search-results")
            .max_height(144.0)
            .show_rows(ui, row_height, self.search_results.len(), |ui, range| {
                let mut selected = None;
                for index in range {
                    let id = self.search_results[index];
                    let response = ui.add_sized(
                        [ui.available_width(), row_height],
                        egui::Button::selectable(
                            self.selection == Selection::Node(id),
                            &document.graph.nodes[id].name,
                        )
                        .truncate(),
                    );
                    if response.clicked() {
                        selected = Some(id);
                    }
                }
                selected
            });
        if let Some(id) = output.inner {
            self.select(Selection::Node(id), true);
        }
        Some(output)
    }

    fn navigation(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "数学图谱", "MATHLIB / IMPORT EXPLORER");
        if let Some(document) = self.document.clone() {
            ui.label(
                RichText::new(format!("{} 个模块", document.graph.nodes.len()))
                    .size(24.0)
                    .strong(),
            );
            ui.small(format!(
                "{} 条导入关系 · {} 个主题",
                document.graph.edges.len(),
                document.topics.len()
            ));
            ui.add_space(8.0);
            if theme::button(ui, "全部模块", false, true).clicked() {
                self.select(Selection::None, false);
            }
            theme::section(ui, "主题导航", "选择一个领域查看关联");
            ui.add(
                egui::TextEdit::singleline(&mut self.topic_filter)
                    .hint_text("筛选主题")
                    .desired_width(f32::INFINITY),
            );
            let filter = self.topic_filter.trim().to_lowercase();
            egui::ScrollArea::vertical()
                .id_salt("topic-navigation")
                .show(ui, |ui| {
                    for (topic, count, color) in &document.topics {
                        if !topic.to_lowercase().contains(&filter) {
                            continue;
                        }
                        let chosen =
                            matches!(&self.selection, Selection::Topic(value) if value == topic);
                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(6.0, 6.0), egui::Sense::hover());
                            ui.painter().circle_filled(
                                rect.center(),
                                3.0,
                                Color32::from_rgb(color.r, color.g, color.b),
                            );
                            if ui
                                .selectable_label(chosen, format!("{topic}  {count}"))
                                .clicked()
                            {
                                self.select(Selection::Topic(topic.clone()), false);
                            }
                        });
                    }
                });
        } else {
            ui.label("从内置图或本地 Mathlib 源码开始探索.");
            if theme::button(ui, "选择数据源", true, self.pending.is_none()).clicked() {
                self.data_open = true;
            }
        }
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "模块检查器", "INSPECT / RELATIONSHIPS");
        let Some(document) = self.document.clone() else {
            ui.label("尚未载入数据");
            return;
        };
        egui::ScrollArea::vertical()
            .id_salt("inspector-scroll")
            .show(ui, |ui| {
                match &self.selection {
                    Selection::None => {
                        ui.add_space(16.0);
                        ui.label(RichText::new("选择节点, 追踪依赖").size(18.0).strong());
                        ui.label("点击图中模块、主题标签或顶部搜索结果, 在这里查看结构关系.");
                        ui.add_space(16.0);
                        ui.small("2D 保留精确拓扑顺序. 3D 用于空间探索, 不改变导入方向.");
                    }
                    Selection::Node(id) => {
                        ui.label(RichText::new(&document.graph.nodes[*id].name).strong());
                        ui.small(&document.graph.nodes[*id].topic);
                    }
                    Selection::Topic(topic) => {
                        ui.heading(topic);
                    }
                }
                if self.selection != Selection::None {
                    let modes = if self.mode.is_topic() {
                        [
                            HighlightMode::TopicMembers,
                            HighlightMode::TopicDependencies,
                            HighlightMode::TopicDependents,
                        ]
                    } else {
                        [
                            HighlightMode::Neighbors,
                            HighlightMode::Dependencies,
                            HighlightMode::Dependents,
                        ]
                    };
                    theme::section(ui, "关系视角", "同一选择同步到 2D 与 3D");
                    let old = self.mode;
                    for mode in modes {
                        ui.selectable_value(&mut self.mode, mode, mode.label());
                    }
                    if old != self.mode {
                        self.select(self.selection.clone(), false);
                    }
                    ui.small(format!("高亮 {} 个模块", self.highlighted_count));
                    if theme::button(ui, "清除选择", false, true).clicked() {
                        self.select(Selection::None, false);
                    }
                }
                if let Selection::Node(id) = self.selection {
                    if theme::button(ui, "定位此模块", true, true).clicked() {
                        self.view.focus_node(id);
                        if let Some(view) = &mut self.nebula {
                            view.focus_node(id);
                        }
                    }
                    for (label, ids) in [
                        ("直接依赖", &document.graph.dependencies[id]),
                        ("直接使用者", &document.graph.dependents[id]),
                    ] {
                        theme::section(ui, label, &format!("{} 个模块", ids.len()));
                        for &neighbor in ids {
                            if ui
                                .selectable_label(false, &document.graph.nodes[neighbor].name)
                                .clicked()
                            {
                                self.select(Selection::Node(neighbor), true);
                            }
                        }
                    }
                }
                theme::section(ui, "视图显示", "不修改图数据或导出布局");
                ui.checkbox(&mut self.options.show_edges, "显示连边");
                ui.checkbox(&mut self.options.show_topic_labels, "显示主题提示");
                ui.checkbox(&mut self.options.show_node_labels, "显示模块标签");
            });
    }

    fn dialogs(&mut self, context: &egui::Context) {
        let mut open = self.data_open;
        egui::Window::new("数据来源")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(460.0)
            .show(context, |ui| {
                ui.add_enabled_ui(self.pending.is_none(), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.selectable_value(&mut self.source_kind, SourceKind::Bundled, "内置图");
                        ui.selectable_value(&mut self.source_kind, SourceKind::Graph, "文本图");
                        ui.selectable_value(
                            &mut self.source_kind,
                            SourceKind::Mathlib,
                            "Mathlib 源码",
                        );
                    });
                    if self.source_kind == SourceKind::Bundled {
                        ui.label("data/import_graph.txt · 随程序内置, 不依赖工作目录");
                    } else {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.source_path)
                                .hint_text("文件或目录路径")
                                .desired_width(f32::INFINITY),
                        );
                    }
                    if theme::button(ui, "载入图谱", true, true).clicked() {
                        if self.source_kind != SourceKind::Bundled
                            && self.source_path.trim().is_empty()
                        {
                            self.error = Some("请填写图文件或 Mathlib 目录路径".into());
                        } else {
                            let source = match self.source_kind {
                                SourceKind::Bundled => Source::Bundled,
                                SourceKind::Graph => {
                                    Source::Graph(PathBuf::from(&self.source_path))
                                }
                                SourceKind::Mathlib => {
                                    Source::Mathlib(PathBuf::from(&self.source_path))
                                }
                            };
                            self.start_load(source, context);
                            self.data_open = false;
                        }
                    }
                });
                if let Some(document) = &self.document {
                    ui.separator();
                    ui.small(format!("当前: {}", document.source.path().display()));
                    if !document.warnings.is_empty() {
                        ui.collapsing(format!("导入说明 ({})", document.warnings.len()), |ui| {
                            for warning in &document.warnings {
                                ui.small(warning);
                            }
                        });
                    }
                }
            });
        self.data_open &= open;
        let mut open = self.export_open;
        egui::Window::new("导出当前图")
            .open(&mut open)
            .collapsible(false)
            .default_width(460.0)
            .show(context, |ui| {
                ui.label("保存当前拓扑图和原始 2D 坐标. 3D 相机不会改变导出数据.");
                ui.add(
                    egui::TextEdit::singleline(&mut self.export_path)
                        .hint_text("尚不存在的新 .txt 文件路径")
                        .desired_width(f32::INFINITY),
                );
                ui.small("拒绝覆盖已有文件和符号链接.");
                if theme::button(
                    ui,
                    "导出新文件",
                    true,
                    self.document.is_some() && self.pending.is_none(),
                )
                .clicked()
                {
                    self.start_export(context);
                    self.export_open = false;
                }
            });
        self.export_open &= open;
    }

    fn workbench(&mut self, ui: &mut egui::Ui) {
        let size = theme::workbench_size(ui.available_width());
        egui::Panel::top("command-bar")
            .frame(theme::panel_frame())
            .show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status-bar")
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::symmetric(16, 8)),
            )
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if self.pending.is_some() {
                        ui.spinner();
                        ui.label(self.task_label);
                    } else {
                        ui.label(RichText::new("●").color(theme::SUCCESS));
                        ui.small("本地运行 · 无外部程序");
                    }
                    if let Some(error) = &self.error {
                        ui.colored_label(Color32::LIGHT_RED, format!("错误: {error}"));
                    } else if let Some(notice) = &self.notice {
                        ui.small(notice);
                    }
                });
                ui.small(match self.view_mode {
                    ViewMode::Topology => "2D · 滚轮缩放 / 拖拽平移 / 依赖项 → 使用者",
                    ViewMode::Nebula => {
                        "3D · 左键旋转 / Shift 或中键平移 / 滚轮缩放 / X 轴为拓扑方向"
                    }
                });
            });
        if size == theme::WorkbenchSize::Wide || self.navigation_open {
            egui::Panel::left("navigation-rail")
                .default_size(if size == theme::WorkbenchSize::Wide {
                    248.0
                } else {
                    208.0
                })
                .min_size(180.0)
                .max_size(340.0)
                .frame(theme::panel_frame())
                .show(ui, |ui| self.navigation(ui));
        }
        if size == theme::WorkbenchSize::Wide {
            egui::Panel::right("module-inspector")
                .default_size(300.0)
                .min_size(240.0)
                .max_size(460.0)
                .frame(theme::panel_frame())
                .show(ui, |ui| self.inspector(ui));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::CANVAS))
            .show(ui, |ui| {
                if let (Some(document), Some(mask)) = (&self.document, &self.highlighted) {
                    let events = match self.view_mode {
                        ViewMode::Topology => self.view.show(
                            ui,
                            &document.graph,
                            &document.layout,
                            mask,
                            &self.selection,
                            &self.options,
                        ),
                        ViewMode::Nebula => {
                            if let Some(view) = &mut self.nebula {
                                view.show(
                                    ui,
                                    &document.graph,
                                    &document.layout,
                                    mask,
                                    &self.selection,
                                    &self.options,
                                )
                            } else {
                                ui.centered_and_justified(|ui| {
                                    ui.colored_label(
                                        Color32::LIGHT_RED,
                                        "3D 渲染不可用: 未获得 wgpu 设备. 2D 视图仍可使用.",
                                    );
                                });
                                Vec::new()
                            }
                        }
                    };
                    for event in events {
                        match event {
                            ViewEvent::SelectNode(id) => self.select(Selection::Node(id), false),
                            ViewEvent::SelectTopic(topic) => {
                                self.select(Selection::Topic(topic), false)
                            }
                            ViewEvent::ClearSelection => self.select(Selection::None, false),
                        }
                    }
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new(if self.pending.is_some() {
                                "正在准备图谱…"
                            } else {
                                "选择数据源, 开始探索数学结构"
                            })
                            .size(20.0)
                            .color(theme::MUTED),
                        );
                    });
                }
            });
        if size == theme::WorkbenchSize::Compact && self.inspector_open {
            let mut open = self.inspector_open;
            egui::Window::new("模块检查器")
                .open(&mut open)
                .default_width(300.0)
                .default_pos([320.0, 120.0])
                .show(ui.ctx(), |ui| self.inspector(ui));
            self.inspector_open &= open;
        }
        self.dialogs(ui.ctx());
    }
}

impl eframe::App for Explorer {
    fn logic(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_task(context);
        if self.pending.is_some() && context.input(|input| input.viewport().close_requested()) {
            context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.notice = Some("后台任务正在完成, 完成后可关闭窗口".into());
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.workbench(ui);
    }
}

/// 直接读取系统中文字体, 不调用 fontconfig 命令或复制原项目字体.
fn install_cjk_font(context: &egui::Context) {
    for path in [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "system-cjk".into(),
                Arc::new(egui::FontData::from_owned(bytes)),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("system-cjk".into());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("system-cjk".into());
            context.set_fonts(fonts);
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_search_and_highlight_are_shared_by_both_view_modes() {
        let document = document::load(Source::Bundled).unwrap();
        let mut app = Explorer::empty(None);
        app.highlighted = Some(Highlight::all(&document.graph));
        app.document = Some(Arc::new(document));
        app.search = "LinearAlgebra.Basis".into();
        app.rebuild_search();
        let id = app.search_results[0];
        app.select(Selection::Node(id), false);
        let mask = app.highlighted.clone().unwrap();
        app.view_mode = ViewMode::Nebula;
        assert_eq!(app.selection, Selection::Node(id));
        assert_eq!(app.highlighted.as_ref().unwrap(), &mask);
        app.view_mode = ViewMode::Topology;
        assert_eq!(app.selection, Selection::Node(id));
        app.select(Selection::Topic("LinearAlgebra".into()), false);
        assert!(app.mode.is_topic());
        assert!(app.inspector_open);
    }
    #[test]
    fn search_scroll_reaches_and_clicks_last_of_more_than_twenty_four_results() {
        use mathseea_model::{ImportGraph, Layout, ModuleNode, NodePlacement, Point};
        let graph = ImportGraph::new(
            (0..64)
                .map(|id| ModuleNode::new(format!("Mathlib.Search.Item{id:02}")))
                .collect(),
            Vec::new(),
        )
        .unwrap();
        let layout = Layout::from_nodes(
            &graph,
            (0..64)
                .map(|id| NodePlacement {
                    position: Point::new(id as f32, 0.0),
                    radius: 1.0,
                })
                .collect(),
        )
        .unwrap();
        let last = graph.nodes.len() - 1;
        let target_name = graph.nodes[last].name.clone();
        let document = LoadedGraph {
            search_names: graph.nodes.iter().map(|n| n.name.to_lowercase()).collect(),
            graph,
            layout,
            source: Source::Bundled,
            warnings: Vec::new(),
            topics: Vec::new(),
        };
        let context = egui::Context::default();
        theme::apply(&context);
        let mut app = Explorer::empty(None);
        app.document = Some(Arc::new(document));
        app.search = "Search.Item".into();
        app.rebuild_search();
        assert!(app.search_results.len() > 24);
        let mut time = 0.0;
        let mut render = |events: Vec<egui::Event>| {
            time += 1.0 / 60.0;
            let mut area = None;
            let output = context.run_ui(
                egui::RawInput {
                    time: Some(time),
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 400.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        area = app
                            .search_results_ui(ui)
                            .map(|o| (o.inner_rect, o.state.offset));
                    });
                },
            );
            let target = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == target_name => {
                    let bounds = egui::Rect::from_min_size(text.pos, text.galley.size())
                        .intersect(shape.clip_rect);
                    bounds.is_positive().then_some(bounds.center())
                }
                _ => None,
            });
            output.drop_without_applying_deltas();
            (area.unwrap(), target)
        };
        let ((bounds, _), first) = render(Vec::new());
        assert!(first.is_none());
        render(vec![
            egui::Event::PointerMoved(bounds.center()),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -6000.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        let mut last_position = None;
        for _ in 0..32 {
            let ((_, offset), position) = render(Vec::new());
            if let Some(position) = position {
                assert!(offset.y > 0.0);
                last_position = Some(position);
                break;
            }
        }
        let position = last_position.expect("last search row must become visible after scrolling");
        for pressed in [true, false] {
            render(vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        assert_eq!(app.selection, Selection::Node(last));
        assert!(app.highlighted.as_ref().unwrap().nodes[last]);
    }

    #[test]
    fn workbench_builds_at_narrow_wide_and_high_dpi_sizes_without_gpu() {
        let context = egui::Context::default();
        theme::apply(&context);
        let mut app = Explorer::empty(None);
        for (width, dpi) in [(640.0, 1.0), (1440.0, 1.0), (1440.0, 2.0)] {
            context.set_pixels_per_point(dpi);
            for view in [ViewMode::Topology, ViewMode::Nebula] {
                app.view_mode = view;
                let output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 800.0),
                        )),
                        ..Default::default()
                    },
                    |ui| app.workbench(ui),
                );
                assert!(!output.shapes.is_empty());
                output.drop_without_applying_deltas();
            }
        }
    }
}
