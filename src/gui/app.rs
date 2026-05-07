use chrono::{DateTime, Local};
use egui::{Color32, FontId, Key, KeyboardShortcut, Modifiers, RichText, ScrollArea, Stroke};
use serialport::{DataBits, FlowControl, Parity, StopBits};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::serial_worker::{
    SerialCommand, SerialConfig, SerialEvent, SerialHandle, spawn_serial_worker,
};

const COMMON_BAUD_RATES: &[u32] = &[
    300, 1200, 2400, 4800, 9600, 14400, 19200, 28800, 38400, 57600, 115200, 230400, 460800, 921600,
];

const MAX_ENTRIES: usize = 5000;
const PORT_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

const DEFAULT_LOG_FONT_SIZE: f32 = 14.0;
const MIN_LOG_FONT_SIZE: f32 = 8.0;
const MAX_LOG_FONT_SIZE: f32 = 32.0;
const LOG_FONT_STEP: f32 = 1.0;

#[derive(Copy, Clone, PartialEq, Eq)]
enum SendMode {
    Ascii,
    Hex,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum LineEnding {
    None,
    Cr,
    Lf,
    CrLf,
}

impl LineEnding {
    fn bytes(self) -> &'static [u8] {
        match self {
            LineEnding::None => b"",
            LineEnding::Cr => b"\r",
            LineEnding::Lf => b"\n",
            LineEnding::CrLf => b"\r\n",
        }
    }

    fn label(self) -> &'static str {
        match self {
            LineEnding::None => "None",
            LineEnding::Cr => "CR (\\r)",
            LineEnding::Lf => "LF (\\n)",
            LineEnding::CrLf => "CRLF (\\r\\n)",
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum DisplayMode {
    Ascii,
    Hex,
    HexDump,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum EntryKind {
    Rx,
    Tx,
    System,
    SystemError,
}

struct LogEntry {
    timestamp: DateTime<Local>,
    kind: EntryKind,
    bytes: Vec<u8>,
    message: Option<String>,
}

pub struct SerialSession {
    id: u64,

    available_ports: Vec<String>,
    last_port_refresh: Instant,

    selected_port: String,
    baud_rate: u32,
    custom_baud_str: String,
    use_custom_baud: bool,
    data_bits: DataBits,
    stop_bits: StopBits,
    parity: Parity,
    flow_control: FlowControl,

    connected: bool,
    connecting: bool,

    send_input: String,
    send_mode: SendMode,
    line_ending: LineEnding,

    entries: Vec<LogEntry>,
    display_mode: DisplayMode,
    show_timestamps: bool,
    auto_scroll: bool,
    show_tx_in_log: bool,
    require_ctrl_enter_to_send: bool,

    bytes_rx: u64,
    bytes_tx: u64,

    status: String,

    serial: SerialHandle,
}

/// Direction in which a `Container`'s children are arranged.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Direction {
    /// Children laid out side-by-side (split-right).
    Horizontal,
    /// Children stacked top-to-bottom (split-down).
    Vertical,
}

/// Tree of split panes. A `Leaf` is a single serial session; a `Container`
/// arranges N children in one direction. Splitting a leaf inside a container
/// of the same direction appends a sibling, so the tree stays flat when it
/// can — only direction changes introduce nesting.
pub enum LayoutNode {
    Leaf(SerialSession),
    Container {
        dir: Direction,
        children: Vec<LayoutNode>,
    },
}

pub struct Tab {
    id: u64,
    title: String,
    root: LayoutNode,
    /// Path through `Container.children` indices to the active leaf.
    /// Empty path means the root itself is the active leaf.
    active_path: Vec<usize>,
}

pub struct GuiApp {
    tabs: Vec<Tab>,
    active_tab: usize,
    next_session_id: u64,
    next_tab_id: u64,
    log_font_size: f32,
    visuals_initialized: bool,
    egui_ctx: egui::Context,
}

impl GuiApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let egui_ctx = cc.egui_ctx.clone();
        let mut app = Self {
            tabs: Vec::new(),
            active_tab: 0,
            next_session_id: 0,
            next_tab_id: 0,
            log_font_size: DEFAULT_LOG_FONT_SIZE,
            visuals_initialized: false,
            egui_ctx,
        };
        let first_tab = app.new_tab_with_one_session("Tab 1".to_string());
        app.tabs.push(first_tab);
        app
    }

    fn new_session(&mut self) -> SerialSession {
        let id = self.next_session_id;
        self.next_session_id += 1;
        SerialSession::new(id, self.egui_ctx.clone())
    }

    fn new_tab_with_one_session(&mut self, title: String) -> Tab {
        let session = self.new_session();
        let tab_id = self.next_tab_id;
        self.next_tab_id += 1;
        Tab {
            id: tab_id,
            title,
            root: LayoutNode::Leaf(session),
            active_path: Vec::new(),
        }
    }

    fn add_tab(&mut self) {
        let title = format!("Tab {}", self.tabs.len() + 1);
        let tab = self.new_tab_with_one_session(title);
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
    }

    fn close_tab(&mut self, idx: usize) {
        if self.tabs.len() <= 1 || idx >= self.tabs.len() {
            return;
        }
        // shut down all sessions in the tab
        for_each_leaf(&self.tabs[idx].root, &mut |s| {
            let _ = s.serial.cmd_tx.send(SerialCommand::Shutdown);
        });
        self.tabs.remove(idx);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
    }

    fn split_active_pane(&mut self, dir: Direction) {
        let new_session = self.new_session();
        let tab = &mut self.tabs[self.active_tab];
        let path = tab.active_path.clone();
        let new_path = split_at_path(&mut tab.root, &path, new_session, dir);
        tab.active_path = new_path;
    }

    fn close_pane(&mut self, tab_idx: usize, path: &[usize]) {
        if tab_idx >= self.tabs.len() {
            return;
        }
        let tab = &mut self.tabs[tab_idx];
        if count_leaves(&tab.root) <= 1 {
            return;
        }
        if let Some(removed) = remove_leaf_at_path(&mut tab.root, path) {
            let _ = removed.serial.cmd_tx.send(SerialCommand::Shutdown);
            collapse_single_children(&mut tab.root);
            // Active path may now be invalid — reset to the first leaf in the tree.
            tab.active_path = first_leaf_path(&tab.root);
        }
    }

    fn active_session_mut(&mut self) -> Option<&mut SerialSession> {
        let tab = self.tabs.get_mut(self.active_tab)?;
        let path = tab.active_path.clone();
        leaf_at_path_mut(&mut tab.root, &path)
    }

    fn save_active_log_to_timestamped_file(&mut self) {
        let Some(session) = self.active_session_mut() else {
            return;
        };
        session.save_log_to_timestamped_file();
    }

    fn bump_log_font(&mut self, delta: f32) {
        self.log_font_size =
            (self.log_font_size + delta).clamp(MIN_LOG_FONT_SIZE, MAX_LOG_FONT_SIZE);
    }

    fn reset_log_font(&mut self) {
        self.log_font_size = DEFAULT_LOG_FONT_SIZE;
    }

    fn handle_global_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::S)))
        {
            self.save_active_log_to_timestamped_file();
        }
        if ctx.input_mut(|i| {
            i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Plus))
                || i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Equals))
        }) {
            self.bump_log_font(LOG_FONT_STEP);
        }
        if ctx.input_mut(|i| {
            i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Minus))
        }) {
            self.bump_log_font(-LOG_FONT_STEP);
        }
        if ctx.input_mut(|i| {
            i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Num0))
        }) {
            self.reset_log_font();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::T)))
        {
            self.add_tab();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::W)))
        {
            self.close_tab(self.active_tab);
        }
    }

    fn render_tab_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let mut close_idx: Option<usize> = None;
            let multi_tab = self.tabs.len() > 1;
            for i in 0..self.tabs.len() {
                let is_active = i == self.active_tab;
                let title = self.tabs[i].title.clone();
                ui.scope(|ui| {
                    if is_active {
                        ui.visuals_mut().widgets.inactive.weak_bg_fill =
                            Color32::from_rgb(220, 230, 245);
                    }
                    if ui
                        .selectable_label(is_active, format!("  {}  ", title))
                        .clicked()
                    {
                        self.active_tab = i;
                    }
                    if multi_tab {
                        if ui.small_button("✕").on_hover_text("Close tab").clicked() {
                            close_idx = Some(i);
                        }
                    }
                });
                ui.add_space(2.0);
            }
            if let Some(i) = close_idx {
                self.close_tab(i);
            }
            ui.separator();
            if ui
                .button("+ New tab")
                .on_hover_text("New tab (Ctrl+T)")
                .clicked()
            {
                self.add_tab();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("Log font: {:.0} px", self.log_font_size))
                        .small()
                        .color(Color32::GRAY),
                );
                if ui
                    .small_button("A↺")
                    .on_hover_text("Reset log font (Ctrl+0)")
                    .clicked()
                {
                    self.reset_log_font();
                }
                if ui
                    .small_button("A−")
                    .on_hover_text("Smaller log font (Ctrl+-)")
                    .clicked()
                {
                    self.bump_log_font(-LOG_FONT_STEP);
                }
                if ui
                    .small_button("A+")
                    .on_hover_text("Larger log font (Ctrl+=)")
                    .clicked()
                {
                    self.bump_log_font(LOG_FONT_STEP);
                }
            });
        });
    }

    fn render_active_tab(&mut self, ui: &mut egui::Ui) {
        let log_font_size = self.log_font_size;
        let active_tab = self.active_tab;
        let tab = &mut self.tabs[active_tab];
        let tab_id = tab.id;
        let multi_pane = count_leaves(&tab.root) > 1;
        let active_path = tab.active_path.clone();

        let mut collected = CollectedActions::default();
        render_node(
            &mut tab.root,
            ui,
            Vec::new(),
            tab_id,
            &active_path,
            log_font_size,
            multi_pane,
            &mut collected,
        );

        if let Some(path) = collected.activate {
            tab.active_path = path;
        }
        if let Some((path, dir)) = collected.split {
            // Make sure the active pointer matches the pane we're splitting from,
            // so split_active_pane operates on the right leaf even if the user
            // clicked Split without first clicking the pane body.
            self.tabs[active_tab].active_path = path;
            self.split_active_pane(dir);
        } else if let Some(path) = collected.close {
            self.close_pane(active_tab, &path);
        }
    }

    fn any_pane_busy(&self) -> bool {
        self.tabs.iter().any(|t| {
            let mut busy = false;
            for_each_leaf(&t.root, &mut |s| {
                if s.connected || s.connecting {
                    busy = true;
                }
            });
            busy
        })
    }
}

#[derive(Default)]
struct PaneAction {
    split_right: bool,
    split_down: bool,
    close: bool,
}

#[derive(Default)]
struct CollectedActions {
    activate: Option<Vec<usize>>,
    split: Option<(Vec<usize>, Direction)>,
    close: Option<Vec<usize>>,
}

#[allow(clippy::too_many_arguments)]
fn render_node(
    node: &mut LayoutNode,
    ui: &mut egui::Ui,
    path: Vec<usize>,
    tab_id: u64,
    active_path: &[usize],
    log_font_size: f32,
    multi_pane: bool,
    actions: &mut CollectedActions,
) {
    match node {
        LayoutNode::Leaf(session) => {
            let is_active = path.as_slice() == active_path;
            let pane_action = session.ui(ui, log_font_size, multi_pane, is_active);
            if pane_action.split_right {
                actions.split = Some((path.clone(), Direction::Horizontal));
            } else if pane_action.split_down {
                actions.split = Some((path.clone(), Direction::Vertical));
            }
            if pane_action.close {
                actions.close = Some(path.clone());
            }
            if ui.input(|inp| inp.pointer.any_click()) && ui.ui_contains_pointer() {
                actions.activate = Some(path);
            }
        }
        LayoutNode::Container { dir, children } => {
            let dir = *dir;
            let n = children.len();
            if n == 0 {
                return;
            }
            for i in 0..n.saturating_sub(1) {
                let mut child_path = path.clone();
                child_path.push(i);
                let panel_id = format!("split_{}_{:?}", tab_id, child_path);
                let avail = match dir {
                    Direction::Horizontal => ui.available_width(),
                    Direction::Vertical => ui.available_height(),
                };
                let default_size = avail / (n - i) as f32;
                match dir {
                    Direction::Horizontal => {
                        egui::SidePanel::left(panel_id)
                            .resizable(true)
                            .default_width(default_size)
                            .min_width(220.0)
                            .show_inside(ui, |ui| {
                                render_node(
                                    &mut children[i],
                                    ui,
                                    child_path,
                                    tab_id,
                                    active_path,
                                    log_font_size,
                                    multi_pane,
                                    actions,
                                );
                            });
                    }
                    Direction::Vertical => {
                        egui::TopBottomPanel::top(panel_id)
                            .resizable(true)
                            .default_height(default_size)
                            .min_height(140.0)
                            .show_inside(ui, |ui| {
                                render_node(
                                    &mut children[i],
                                    ui,
                                    child_path,
                                    tab_id,
                                    active_path,
                                    log_font_size,
                                    multi_pane,
                                    actions,
                                );
                            });
                    }
                }
            }
            let last = n - 1;
            let mut last_path = path.clone();
            last_path.push(last);
            egui::CentralPanel::default()
                .frame(egui::Frame::central_panel(ui.style()).inner_margin(0.0))
                .show_inside(ui, |ui| {
                    render_node(
                        &mut children[last],
                        ui,
                        last_path,
                        tab_id,
                        active_path,
                        log_font_size,
                        multi_pane,
                        actions,
                    );
                });
        }
    }
}

// ---- Tree helpers ----

fn count_leaves(node: &LayoutNode) -> usize {
    match node {
        LayoutNode::Leaf(_) => 1,
        LayoutNode::Container { children, .. } => children.iter().map(count_leaves).sum(),
    }
}

fn for_each_leaf<F: FnMut(&SerialSession)>(node: &LayoutNode, f: &mut F) {
    match node {
        LayoutNode::Leaf(s) => f(s),
        LayoutNode::Container { children, .. } => {
            for c in children {
                for_each_leaf(c, f);
            }
        }
    }
}

fn navigate_mut<'a>(node: &'a mut LayoutNode, path: &[usize]) -> Option<&'a mut LayoutNode> {
    let mut cur = node;
    for &idx in path {
        cur = match cur {
            LayoutNode::Container { children, .. } => children.get_mut(idx)?,
            LayoutNode::Leaf(_) => return None,
        };
    }
    Some(cur)
}

fn leaf_at_path_mut<'a>(node: &'a mut LayoutNode, path: &[usize]) -> Option<&'a mut SerialSession> {
    match navigate_mut(node, path)? {
        LayoutNode::Leaf(s) => Some(s),
        _ => None,
    }
}

fn first_leaf_path(node: &LayoutNode) -> Vec<usize> {
    let mut path = Vec::new();
    let mut cur = node;
    while let LayoutNode::Container { children, .. } = cur {
        if children.is_empty() {
            return path;
        }
        path.push(0);
        cur = &children[0];
    }
    path
}

/// Insert a new leaf next to the leaf at `path`, splitting in `dir`.
/// If the parent already arranges children in `dir`, the new leaf is appended
/// as a sibling at `path[last]+1`. Otherwise the leaf at `path` is replaced
/// with a new container of `dir` containing the original leaf followed by the
/// new one. Returns the path to the newly inserted leaf.
fn split_at_path(
    root: &mut LayoutNode,
    path: &[usize],
    new_session: SerialSession,
    dir: Direction,
) -> Vec<usize> {
    if path.is_empty() {
        // Splitting the root leaf: replace root with a container holding [old, new].
        // Take a placeholder container to swap into root, then put old leaf inside.
        let old = std::mem::replace(
            root,
            LayoutNode::Container {
                dir,
                children: Vec::new(),
            },
        );
        if let LayoutNode::Container { children, .. } = root {
            children.push(old);
            children.push(LayoutNode::Leaf(new_session));
        }
        return vec![1];
    }

    let parent_path = &path[..path.len() - 1];
    let last_idx = path[path.len() - 1];

    let Some(parent) = navigate_mut(root, parent_path) else {
        return path.to_vec();
    };

    let LayoutNode::Container {
        dir: parent_dir,
        children,
    } = parent
    else {
        return path.to_vec();
    };

    if *parent_dir == dir {
        // Append as sibling — keeps the existing direction flat.
        children.insert(last_idx + 1, LayoutNode::Leaf(new_session));
        let mut new_path = parent_path.to_vec();
        new_path.push(last_idx + 1);
        new_path
    } else {
        // Wrap the leaf at this position in a new container of the new direction.
        let old = std::mem::replace(
            &mut children[last_idx],
            LayoutNode::Container {
                dir,
                children: Vec::new(),
            },
        );
        if let LayoutNode::Container {
            children: inner, ..
        } = &mut children[last_idx]
        {
            inner.push(old);
            inner.push(LayoutNode::Leaf(new_session));
        }
        let mut new_path = path.to_vec();
        new_path.push(1);
        new_path
    }
}

/// Remove the leaf at `path` and return the removed session. Does NOT collapse
/// single-child containers — call `collapse_single_children` afterwards.
fn remove_leaf_at_path(root: &mut LayoutNode, path: &[usize]) -> Option<SerialSession> {
    if path.is_empty() {
        // Caller is responsible for not removing the only leaf.
        return None;
    }
    let parent_path = &path[..path.len() - 1];
    let last_idx = path[path.len() - 1];
    let parent = navigate_mut(root, parent_path)?;
    let LayoutNode::Container { children, .. } = parent else {
        return None;
    };
    if last_idx >= children.len() {
        return None;
    }
    match children.remove(last_idx) {
        LayoutNode::Leaf(s) => Some(s),
        // Removing a non-leaf via this path isn't expected from the UI; if it
        // somehow happened, the subtree is just dropped (sessions inside it
        // are not shut down). The UI never produces such a request.
        _ => None,
    }
}

/// Walks the tree and replaces any `Container` with exactly one child by that
/// child. Repeats until stable, including cascading collapses up the spine.
fn collapse_single_children(node: &mut LayoutNode) {
    loop {
        let LayoutNode::Container { children, .. } = node else {
            break;
        };
        if children.len() != 1 {
            break;
        }
        let only = children.pop().unwrap();
        *node = only;
    }
    if let LayoutNode::Container { children, .. } = node {
        for c in children.iter_mut() {
            collapse_single_children(c);
        }
    }
}

impl SerialSession {
    fn new(id: u64, egui_ctx: egui::Context) -> Self {
        let serial = spawn_serial_worker(egui_ctx);
        let available_ports = list_ports();
        let selected_port = available_ports.first().cloned().unwrap_or_default();
        Self {
            id,
            available_ports,
            last_port_refresh: Instant::now(),
            selected_port,
            baud_rate: 115200,
            custom_baud_str: "115200".to_string(),
            use_custom_baud: false,
            data_bits: DataBits::Eight,
            stop_bits: StopBits::One,
            parity: Parity::None,
            flow_control: FlowControl::None,
            connected: false,
            connecting: false,
            send_input: String::new(),
            send_mode: SendMode::Ascii,
            line_ending: LineEnding::CrLf,
            entries: Vec::new(),
            display_mode: DisplayMode::Ascii,
            show_timestamps: true,
            auto_scroll: true,
            show_tx_in_log: true,
            require_ctrl_enter_to_send: false,
            bytes_rx: 0,
            bytes_tx: 0,
            status: "Idle".to_string(),
            serial,
        }
    }

    fn refresh_ports(&mut self) {
        self.available_ports = list_ports();
        if !self.available_ports.contains(&self.selected_port) {
            self.selected_port = self.available_ports.first().cloned().unwrap_or_default();
        }
    }

    fn drain_serial_events(&mut self) {
        while let Ok(evt) = self.serial.event_rx.try_recv() {
            match evt {
                SerialEvent::Connected { port, baud_rate } => {
                    self.connected = true;
                    self.connecting = false;
                    self.status = format!("Connected to {} @ {}bps", port, baud_rate);
                    self.push_system(format!("Connected to {} @ {}bps", port, baud_rate));
                }
                SerialEvent::Disconnected => {
                    self.connected = false;
                    self.connecting = false;
                    self.status = "Disconnected".to_string();
                    self.push_system("Disconnected".to_string());
                }
                SerialEvent::Error(msg) => {
                    self.connecting = false;
                    self.status = format!("Error: {}", msg);
                    self.push_system_error(msg);
                }
                SerialEvent::RxLine { timestamp, bytes } => {
                    self.bytes_rx += bytes.len() as u64;
                    self.entries.push(LogEntry {
                        timestamp,
                        kind: EntryKind::Rx,
                        bytes,
                        message: None,
                    });
                    self.trim_log();
                }
                SerialEvent::TxEcho { timestamp, bytes } => {
                    self.bytes_tx += bytes.len() as u64;
                    if self.show_tx_in_log {
                        self.entries.push(LogEntry {
                            timestamp,
                            kind: EntryKind::Tx,
                            bytes,
                            message: None,
                        });
                        self.trim_log();
                    }
                }
            }
        }
    }

    fn push_system(&mut self, msg: String) {
        self.entries.push(LogEntry {
            timestamp: Local::now(),
            kind: EntryKind::System,
            bytes: Vec::new(),
            message: Some(msg),
        });
        self.trim_log();
    }

    fn push_system_error(&mut self, msg: String) {
        self.entries.push(LogEntry {
            timestamp: Local::now(),
            kind: EntryKind::SystemError,
            bytes: Vec::new(),
            message: Some(msg),
        });
        self.trim_log();
    }

    fn trim_log(&mut self) {
        if self.entries.len() > MAX_ENTRIES {
            let excess = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(..excess);
        }
    }

    fn build_send_payload(&self) -> Result<Vec<u8>, String> {
        let mut payload = match self.send_mode {
            SendMode::Ascii => self.send_input.as_bytes().to_vec(),
            SendMode::Hex => parse_hex_input(&self.send_input)?,
        };
        payload.extend_from_slice(self.line_ending.bytes());
        Ok(payload)
    }

    fn try_send(&mut self) {
        if !self.connected {
            self.push_system_error("Cannot send: not connected".to_string());
            return;
        }
        match self.build_send_payload() {
            Ok(payload) if payload.is_empty() => {}
            Ok(payload) => {
                let _ = self.serial.cmd_tx.send(SerialCommand::Send(payload));
                self.send_input.clear();
            }
            Err(e) => {
                self.push_system_error(format!("Send error: {}", e));
            }
        }
    }

    fn current_config(&self) -> SerialConfig {
        SerialConfig {
            port: self.selected_port.clone(),
            baud_rate: self.baud_rate,
            data_bits: self.data_bits,
            stop_bits: self.stop_bits,
            parity: self.parity,
            flow_control: self.flow_control,
        }
    }

    fn save_log_to_timestamped_file(&mut self) {
        let filename = format!("{}.txt", Local::now().format("%Y%m%d_%H%M%S"));
        let path = PathBuf::from(&filename);
        let mut content = String::new();
        for entry in &self.entries {
            let ts = format!("[{}] ", entry.timestamp.format("%Y-%m-%d %H:%M:%S%.3f"));
            let prefix = match entry.kind {
                EntryKind::Rx => "RX ",
                EntryKind::Tx => "TX ",
                EntryKind::System => "-- ",
                EntryKind::SystemError => "!! ",
            };
            let body = match entry.kind {
                EntryKind::System | EntryKind::SystemError => {
                    entry.message.clone().unwrap_or_default()
                }
                EntryKind::Rx | EntryKind::Tx => format_bytes(&entry.bytes, self.display_mode),
            };
            content.push_str(&ts);
            content.push_str(prefix);
            content.push_str(&body);
            content.push('\n');
        }
        match std::fs::write(&path, content) {
            Ok(()) => {
                let abs = std::fs::canonicalize(&path)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| filename.clone());
                self.push_system(format!("Saved log to {}", abs));
            }
            Err(e) => {
                self.push_system_error(format!("Failed to save log to {}: {}", filename, e));
            }
        }
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        log_font_size: f32,
        multi_pane: bool,
        is_active: bool,
    ) -> PaneAction {
        // Per-frame housekeeping
        if self.last_port_refresh.elapsed() >= PORT_REFRESH_INTERVAL && !self.connected {
            self.refresh_ports();
            self.last_port_refresh = Instant::now();
        }
        self.drain_serial_events();

        if is_active {
            // Faint highlight on the active pane border
            let rect = ui.max_rect();
            ui.painter().rect_stroke(
                rect.shrink(1.0),
                2.0,
                Stroke::new(1.5, Color32::from_rgb(80, 130, 220)),
            );
        }

        let mut action = PaneAction::default();

        // Use a unique id-scope so widgets in multiple sessions don't collide.
        let session_id = self.id;
        ui.push_id(session_id, |ui| {
            egui::TopBottomPanel::top(format!("settings_{}", session_id))
                .resizable(false)
                .show_inside(ui, |ui| {
                    self.settings_panel(ui, multi_pane, &mut action);
                });
            egui::TopBottomPanel::bottom(format!("status_{}", session_id))
                .resizable(false)
                .show_inside(ui, |ui| {
                    self.status_bar(ui);
                });
            egui::TopBottomPanel::bottom(format!("send_{}", session_id))
                .resizable(true)
                .min_height(80.0)
                .show_inside(ui, |ui| {
                    ui.add_space(2.0);
                    self.send_panel(ui);
                    ui.add_space(2.0);
                });
            egui::CentralPanel::default()
                .frame(egui::Frame::central_panel(ui.style()).inner_margin(2.0))
                .show_inside(ui, |ui| {
                    self.log_toolbar(ui);
                    ui.separator();
                    self.log_view(ui, log_font_size);
                });
        });

        action
    }

    fn settings_panel(&mut self, ui: &mut egui::Ui, multi_pane: bool, action: &mut PaneAction) {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label("Port:");
            let port_label = if self.selected_port.is_empty() {
                "<no ports>".to_string()
            } else {
                self.selected_port.clone()
            };
            egui::ComboBox::from_id_salt("port_combo")
                .selected_text(port_label)
                .show_ui(ui, |ui| {
                    if self.available_ports.is_empty() {
                        ui.label("No serial ports detected");
                    }
                    for p in self.available_ports.clone() {
                        ui.selectable_value(&mut self.selected_port, p.clone(), p);
                    }
                });
            if ui.button("Refresh").clicked() {
                self.refresh_ports();
            }

            ui.separator();

            ui.label("Baud:");
            let baud_text = if self.use_custom_baud {
                "Custom".to_string()
            } else {
                self.baud_rate.to_string()
            };
            egui::ComboBox::from_id_salt("baud_combo")
                .selected_text(baud_text)
                .show_ui(ui, |ui| {
                    for &b in COMMON_BAUD_RATES {
                        if ui
                            .selectable_label(
                                !self.use_custom_baud && self.baud_rate == b,
                                b.to_string(),
                            )
                            .clicked()
                        {
                            self.baud_rate = b;
                            self.custom_baud_str = b.to_string();
                            self.use_custom_baud = false;
                        }
                    }
                    if ui
                        .selectable_label(self.use_custom_baud, "Custom...")
                        .clicked()
                    {
                        self.use_custom_baud = true;
                    }
                });
            if self.use_custom_baud {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.custom_baud_str)
                        .desired_width(80.0)
                        .hint_text("baud"),
                );
                if resp.changed() {
                    if let Ok(b) = self.custom_baud_str.parse::<u32>()
                        && b > 0
                    {
                        self.baud_rate = b;
                    }
                }
            }

            ui.separator();

            let connect_label = if self.connected {
                "Disconnect"
            } else if self.connecting {
                "Connecting..."
            } else {
                "Connect"
            };
            let can_act = !self.selected_port.is_empty() && self.baud_rate > 0;
            let btn = ui.add_enabled(
                can_act && !self.connecting,
                egui::Button::new(connect_label),
            );
            if btn.clicked() {
                if self.connected {
                    let _ = self.serial.cmd_tx.send(SerialCommand::Disconnect);
                } else {
                    self.connecting = true;
                    self.status = format!("Opening {}...", self.selected_port);
                    let cfg = self.current_config();
                    let _ = self.serial.cmd_tx.send(SerialCommand::Connect(cfg));
                }
            }

            ui.separator();
            let (dot, color) = if self.connected {
                ("●", Color32::from_rgb(40, 160, 60))
            } else if self.connecting {
                ("●", Color32::from_rgb(200, 150, 30))
            } else {
                ("●", Color32::from_rgb(190, 60, 60))
            };
            ui.label(RichText::new(dot).color(color));

            // Pane controls on the right
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if multi_pane
                    && ui
                        .small_button("✕ Close pane")
                        .on_hover_text("Close this pane")
                        .clicked()
                {
                    action.close = true;
                }
                if ui
                    .small_button("▼ Split down")
                    .on_hover_text("Open another serial pane below this one")
                    .clicked()
                {
                    action.split_down = true;
                }
                if ui
                    .small_button("▶ Split right")
                    .on_hover_text("Open another serial pane to the right of this one")
                    .clicked()
                {
                    action.split_right = true;
                }
            });
        });

        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label("Data bits:");
            egui::ComboBox::from_id_salt("databits_combo")
                .selected_text(data_bits_label(self.data_bits))
                .show_ui(ui, |ui| {
                    for v in [
                        DataBits::Five,
                        DataBits::Six,
                        DataBits::Seven,
                        DataBits::Eight,
                    ] {
                        ui.selectable_value(&mut self.data_bits, v, data_bits_label(v));
                    }
                });

            ui.separator();
            ui.label("Stop bits:");
            egui::ComboBox::from_id_salt("stopbits_combo")
                .selected_text(stop_bits_label(self.stop_bits))
                .show_ui(ui, |ui| {
                    for v in [StopBits::One, StopBits::Two] {
                        ui.selectable_value(&mut self.stop_bits, v, stop_bits_label(v));
                    }
                });

            ui.separator();
            ui.label("Parity:");
            egui::ComboBox::from_id_salt("parity_combo")
                .selected_text(parity_label(self.parity))
                .show_ui(ui, |ui| {
                    for v in [Parity::None, Parity::Odd, Parity::Even] {
                        ui.selectable_value(&mut self.parity, v, parity_label(v));
                    }
                });

            ui.separator();
            ui.label("Flow:");
            egui::ComboBox::from_id_salt("flow_combo")
                .selected_text(flow_label(self.flow_control))
                .show_ui(ui, |ui| {
                    for v in [
                        FlowControl::None,
                        FlowControl::Software,
                        FlowControl::Hardware,
                    ] {
                        ui.selectable_value(&mut self.flow_control, v, flow_label(v));
                    }
                });

            if self.connected {
                ui.separator();
                if ui.button("Apply").clicked() {
                    let cfg = self.current_config();
                    let _ = self.serial.cmd_tx.send(SerialCommand::Disconnect);
                    self.connecting = true;
                    let _ = self.serial.cmd_tx.send(SerialCommand::Connect(cfg));
                }
            }
        });
        ui.add_space(2.0);
    }

    fn log_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("View:");
            ui.selectable_value(&mut self.display_mode, DisplayMode::Ascii, "ASCII");
            ui.selectable_value(&mut self.display_mode, DisplayMode::Hex, "Hex");
            ui.selectable_value(&mut self.display_mode, DisplayMode::HexDump, "Hex+ASCII");
            ui.separator();
            ui.checkbox(&mut self.show_timestamps, "Timestamps");
            ui.checkbox(&mut self.auto_scroll, "Auto-scroll");
            ui.checkbox(&mut self.show_tx_in_log, "Echo TX");
            ui.separator();
            if ui
                .button("Save…")
                .on_hover_text("Save log to current directory (Ctrl+S)")
                .clicked()
            {
                self.save_log_to_timestamped_file();
            }
            if ui.button("Clear").clicked() {
                self.entries.clear();
                self.bytes_rx = 0;
                self.bytes_tx = 0;
            }
        });
    }

    fn log_view(&self, ui: &mut egui::Ui, log_font_size: f32) {
        let row_height = log_font_size + 4.0;
        let mut area = ScrollArea::vertical().auto_shrink([false, false]);
        if self.auto_scroll {
            area = area.stick_to_bottom(true);
        }
        area.show_rows(ui, row_height, self.entries.len(), |ui, range| {
            for entry in &self.entries[range] {
                self.render_entry(ui, entry, log_font_size);
            }
        });
    }

    fn render_entry(&self, ui: &mut egui::Ui, entry: &LogEntry, log_font_size: f32) {
        let ts = if self.show_timestamps {
            format!("[{}] ", entry.timestamp.format("%H:%M:%S%.3f"))
        } else {
            String::new()
        };

        let (prefix, color) = match entry.kind {
            EntryKind::Rx => ("← ", Color32::from_rgb(10, 60, 170)),
            EntryKind::Tx => ("→ ", Color32::from_rgb(15, 110, 35)),
            EntryKind::System => ("· ", Color32::from_rgb(80, 80, 80)),
            EntryKind::SystemError => ("! ", Color32::from_rgb(190, 30, 30)),
        };

        let body = match entry.kind {
            EntryKind::System | EntryKind::SystemError => entry.message.clone().unwrap_or_default(),
            EntryKind::Rx | EntryKind::Tx => format_bytes(&entry.bytes, self.display_mode),
        };

        let line = format!("{}{}{}", ts, prefix, body);
        ui.label(
            RichText::new(line)
                .color(color)
                .font(FontId::monospace(log_font_size)),
        );
    }

    fn send_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Mode:");
            ui.selectable_value(&mut self.send_mode, SendMode::Ascii, "ASCII");
            ui.selectable_value(&mut self.send_mode, SendMode::Hex, "Hex");
            ui.separator();
            ui.label("Line ending:");
            egui::ComboBox::from_id_salt("line_ending_combo")
                .selected_text(self.line_ending.label())
                .show_ui(ui, |ui| {
                    for v in [
                        LineEnding::None,
                        LineEnding::Cr,
                        LineEnding::Lf,
                        LineEnding::CrLf,
                    ] {
                        ui.selectable_value(&mut self.line_ending, v, v.label());
                    }
                });
            ui.separator();
            ui.checkbox(
                &mut self.require_ctrl_enter_to_send,
                "Require Ctrl+Enter to send",
            )
            .on_hover_text(
                "Off: plain Enter sends. On: Enter inserts a newline; Ctrl+Enter sends.",
            );
        });

        ui.add_space(2.0);

        ui.horizontal(|ui| {
            let hint = match self.send_mode {
                SendMode::Ascii => "Type text to send...",
                SendMode::Hex => "Hex bytes, e.g. A0 B1 0F or A0B10F",
            };
            let edit_id = egui::Id::new(("send_input_edit", self.id));
            let edit_focused = ui.ctx().memory(|m| m.has_focus(edit_id));

            let plain_enter_pressed = edit_focused
                && !self.require_ctrl_enter_to_send
                && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
            let ctrl_enter_pressed =
                edit_focused && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));

            ui.add(
                egui::TextEdit::multiline(&mut self.send_input)
                    .id(edit_id)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .hint_text(hint)
                    .font(egui::TextStyle::Monospace),
            );

            if plain_enter_pressed || ctrl_enter_pressed {
                self.try_send();
            }
        });

        ui.horizontal(|ui| {
            let send_btn = ui.add_enabled(
                self.connected && !self.send_input.is_empty(),
                egui::Button::new("Send"),
            );
            if send_btn.clicked() {
                self.try_send();
            }
            if ui.button("Clear input").clicked() {
                self.send_input.clear();
            }
            if self.send_mode == SendMode::Hex {
                match parse_hex_input(&self.send_input) {
                    Ok(bytes) if !bytes.is_empty() => {
                        ui.label(
                            RichText::new(format!("{} byte(s)", bytes.len()))
                                .small()
                                .color(Color32::GRAY),
                        );
                    }
                    Ok(_) => {}
                    Err(e) => {
                        ui.label(
                            RichText::new(e)
                                .small()
                                .color(Color32::from_rgb(190, 30, 30)),
                        );
                    }
                }
            }
        });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&self.status).small());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("RX: {} B   TX: {} B", self.bytes_rx, self.bytes_tx))
                        .small(),
                );
            });
        });
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.visuals_initialized {
            ctx.set_visuals(egui::Visuals::light());
            self.visuals_initialized = true;
        }

        self.handle_global_shortcuts(ctx);

        egui::TopBottomPanel::top("tab_bar")
            .resizable(false)
            .show(ctx, |ui| self.render_tab_bar(ui));

        egui::CentralPanel::default().show(ctx, |ui| self.render_active_tab(ui));

        if self.any_pane_busy() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        for tab in &self.tabs {
            for_each_leaf(&tab.root, &mut |s| {
                let _ = s.serial.cmd_tx.send(SerialCommand::Shutdown);
            });
        }
    }
}

fn list_ports() -> Vec<String> {
    serialport::available_ports()
        .map(|ports| ports.into_iter().map(|p| p.port_name).collect())
        .unwrap_or_default()
}

fn data_bits_label(d: DataBits) -> &'static str {
    match d {
        DataBits::Five => "5",
        DataBits::Six => "6",
        DataBits::Seven => "7",
        DataBits::Eight => "8",
    }
}

fn stop_bits_label(s: StopBits) -> &'static str {
    match s {
        StopBits::One => "1",
        StopBits::Two => "2",
    }
}

fn parity_label(p: Parity) -> &'static str {
    match p {
        Parity::None => "None",
        Parity::Odd => "Odd",
        Parity::Even => "Even",
    }
}

fn flow_label(f: FlowControl) -> &'static str {
    match f {
        FlowControl::None => "None",
        FlowControl::Software => "XON/XOFF",
        FlowControl::Hardware => "RTS/CTS",
    }
}

fn parse_hex_input(s: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != ':' && *c != '-')
        .collect();
    let cleaned = cleaned
        .strip_prefix("0x")
        .or_else(|| cleaned.strip_prefix("0X"))
        .unwrap_or(&cleaned)
        .to_string();

    if cleaned.is_empty() {
        return Ok(Vec::new());
    }
    if cleaned.len() % 2 != 0 {
        return Err("Hex input must have an even number of digits".to_string());
    }
    let mut out = Vec::with_capacity(cleaned.len() / 2);
    let bytes = cleaned.as_bytes();
    for chunk in bytes.chunks(2) {
        let pair =
            std::str::from_utf8(chunk).map_err(|_| "Invalid characters in hex".to_string())?;
        let b = u8::from_str_radix(pair, 16).map_err(|_| format!("Invalid hex byte: {}", pair))?;
        out.push(b);
    }
    Ok(out)
}

fn format_bytes(bytes: &[u8], mode: DisplayMode) -> String {
    match mode {
        DisplayMode::Ascii => format_ascii(bytes),
        DisplayMode::Hex => format_hex(bytes),
        DisplayMode::HexDump => format_hex_dump(bytes),
    }
}

fn format_ascii(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        match b {
            b'\r' => out.push_str("\\r"),
            b'\n' => out.push_str("\\n"),
            0x09 => out.push('\t'),
            0x20..=0x7E => out.push(b as char),
            _ => out.push_str(&format!("\\x{:02X}", b)),
        }
    }
    out
}

fn format_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{:02X}", b));
    }
    out
}

fn format_hex_dump(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 3);
    let mut ascii = String::with_capacity(bytes.len());
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            hex.push(' ');
        }
        hex.push_str(&format!("{:02X}", b));
        ascii.push(if (0x20..=0x7E).contains(b) {
            *b as char
        } else {
            '.'
        });
    }
    format!("{}  |{}|", hex, ascii)
}
