use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use eframe::egui::{self, Color32, RichText};
use mathseea_graph_engine::highlight;
use mathseea_graph_view::{GraphView, ViewEvent, ViewOptions};
use mathseea_model::{GraphError, Highlight, HighlightMode, NodeId, Selection};

use crate::cli::Source;
use crate::document::{self, LoadedGraph};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    Bundled,
    Graph,
    Mathlib,
}

enum Completed {
    Loaded(Arc<LoadedGraph>),
    Exported(PathBuf),
}

pub struct Explorer {
    document: Option<Arc<LoadedGraph>>,
    view: GraphView,
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
        context.egui_ctx.set_visuals(egui::Visuals::dark());
        install_cjk_font(&context.egui_ctx);
        let mut app = Self {
            document: None,
            view: GraphView::default(),
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
        };
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
                }
                self.highlighted_count = mask.nodes.iter().filter(|&&on| on).count();
                self.highlighted = Some(mask);
                self.selection = selection;
                self.mode = mode;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading("MathSeea");
            ui.separator();
            ui.label("Mathlib 导入图");
            if ui.button("适配视图").clicked() {
                self.view.reset_camera();
            }
            if ui.button("清除选择").clicked() {
                self.select(Selection::None, false);
            }
            ui.separator();
            ui.checkbox(&mut self.options.show_edges, "连边");
            ui.checkbox(&mut self.options.show_topic_labels, "主题标签");
            ui.checkbox(&mut self.options.show_node_labels, "模块标签");
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt("explorer-controls")
            .show(ui, |ui| {
                ui.heading("数据来源");
                ui.add_enabled_ui(self.pending.is_none(), |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.source_kind, SourceKind::Bundled, "内置图");
                        ui.selectable_value(&mut self.source_kind, SourceKind::Graph, "文本图");
                        ui.selectable_value(&mut self.source_kind, SourceKind::Mathlib, "Mathlib 源码");
                    });
                    if self.source_kind == SourceKind::Bundled {
                        ui.small("data/import_graph.txt (随程序内置)");
                    } else {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.source_path)
                                .hint_text("输入文件或目录路径")
                                .desired_width(f32::INFINITY),
                        );
                    }
                    if ui.button("加载").clicked() {
                        if self.source_kind == SourceKind::Bundled {
                            self.start_load(Source::Bundled, ui.ctx());
                        } else if self.source_path.trim().is_empty() {
                            self.error = Some("请填写图文件或 Mathlib 目录路径".into());
                        } else {
                            let path = PathBuf::from(&self.source_path);
                            let source = match self.source_kind {
                                SourceKind::Bundled => Source::Bundled,
                                SourceKind::Graph => Source::Graph(path),
                                SourceKind::Mathlib => Source::Mathlib(path),
                            };
                            self.start_load(source, ui.ctx());
                        }
                    }
                });
                if self.pending.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(self.task_label);
                    });
                }
                let Some(document) = self.document.clone() else {
                    ui.separator();
                    ui.label("加载图文件, 或选择本地 mathlib4 仓库 / Mathlib 目录.");
                    return;
                };
                ui.separator();
                ui.small(format!("当前: {}", document.source.path().display()));
                ui.label(format!(
                    "{} 个模块 / {} 条关系 / {} 个主题",
                    document.graph.nodes.len(),
                    document.graph.edges.len(),
                    document.topics.len()
                ));
                if !document.warnings.is_empty() {
                    ui.collapsing(format!("导入说明 ({})", document.warnings.len()), |ui| {
                        for warning in &document.warnings {
                            ui.small(warning);
                        }
                    });
                }
                ui.separator();
                ui.heading("查找模块");
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("例如 LinearAlgebra.Basis")
                            .desired_width(f32::INFINITY),
                    )
                    .changed()
                {
                    self.rebuild_search();
                }
                if !self.search.trim().is_empty() {
                    ui.small(format!("{} 个匹配", self.search_results.len()));
                    let mut selected = None;
                    egui::ScrollArea::vertical()
                        .id_salt("module-search")
                        .max_height(170.0)
                        .show_rows(ui, 22.0, self.search_results.len(), |ui, range| {
                            for index in range {
                                let id = self.search_results[index];
                                if ui
                                    .selectable_label(
                                        self.selection == Selection::Node(id),
                                        &document.graph.nodes[id].name,
                                    )
                                    .clicked()
                                {
                                    selected = Some(id);
                                }
                            }
                        });
                    if let Some(id) = selected {
                        self.select(Selection::Node(id), true);
                    }
                }
                ui.separator();
                ui.heading("主题");
                egui::ComboBox::from_id_salt("topic-selection")
                    .width(ui.available_width())
                    .selected_text(match &self.selection {
                        Selection::Topic(topic) => topic.as_str(),
                        _ => "选择主题",
                    })
                    .show_ui(ui, |ui| {
                        for (topic, count, color) in &document.topics {
                            let text = RichText::new(format!("{topic} ({count})"))
                                .color(Color32::from_rgb(color.r, color.g, color.b));
                            if ui
                                .selectable_label(
                                    matches!(&self.selection, Selection::Topic(value) if value == topic),
                                    text,
                                )
                                .clicked()
                            {
                                self.select(Selection::Topic(topic.clone()), false);
                            }
                        }
                    });
                self.selection_controls(ui, &document);
                ui.separator();
                ui.collapsing("导出当前图", |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.export_path)
                            .hint_text("新的 .txt 文件路径")
                            .desired_width(f32::INFINITY),
                    );
                    if ui
                        .add_enabled(self.pending.is_none(), egui::Button::new("导出"))
                        .clicked()
                    {
                        self.start_export(ui.ctx());
                    }
                    ui.small("导出到新文件, 保留当前布局.");
                });
            });
    }

    fn selection_controls(&mut self, ui: &mut egui::Ui, document: &LoadedGraph) {
        if self.selection == Selection::None {
            return;
        }
        ui.separator();
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
        let previous = self.mode;
        egui::ComboBox::from_id_salt("highlight-mode")
            .selected_text(self.mode.label())
            .show_ui(ui, |ui| {
                for mode in modes {
                    ui.selectable_value(&mut self.mode, mode, mode.label());
                }
            });
        if self.mode != previous {
            self.select(self.selection.clone(), false);
        }
        ui.small(format!("高亮 {} 个模块", self.highlighted_count));
        if let Selection::Node(id) = self.selection {
            let node = &document.graph.nodes[id];
            ui.label(RichText::new(&node.name).strong());
            ui.small(format!(
                "直接依赖 {} / 直接使用者 {}",
                document.graph.dependencies[id].len(),
                document.graph.dependents[id].len()
            ));
            if ui.button("定位此模块").clicked() {
                self.view.focus_node(id);
            }
            for (label, ids) in [
                ("直接依赖", &document.graph.dependencies[id]),
                ("直接使用者", &document.graph.dependents[id]),
            ] {
                ui.collapsing(format!("{label} ({})", ids.len()), |ui| {
                    for &neighbor in ids {
                        if ui.button(&document.graph.nodes[neighbor].name).clicked() {
                            self.select(Selection::Node(neighbor), true);
                        }
                    }
                });
            }
        }
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
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status").show(ui, |ui| {
            if let Some(error) = &self.error {
                ui.colored_label(Color32::LIGHT_RED, error);
            } else if let Some(notice) = &self.notice {
                ui.label(notice);
            }
            ui.small("滚轮缩放 / 左键或中键拖拽平移 / 点击模块或主题查看关系 / 依赖从左到右");
        });
        egui::Panel::left("sidebar")
            .default_size(320.0)
            .min_size(220.0)
            .max_size(550.0)
            .show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::BLACK))
            .show(ui, |ui| {
                if let (Some(document), Some(mask)) = (&self.document, &self.highlighted) {
                    let events = self.view.show(
                        ui,
                        &document.graph,
                        &document.layout,
                        mask,
                        &self.selection,
                        &self.options,
                    );
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
                            RichText::new("加载 Mathlib 导入图, 探索数学模块之间的联系").size(20.0),
                        );
                    });
                }
            });
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
