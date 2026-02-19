use serialport::{DataBits, FlowControl, Parity, StopBits};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Message {
    // Serial connection commands
    ConnectSerial,
    DisconnectSerial,
    SerialConnected,
    SerialDisconnected,
    
    // Serial configuration
    PortChanged(String),
    BaudrateChanged(String),
    DataBitsChanged(DataBits),
    ParityChanged(Parity),
    StopBitsChanged(StopBits),
    FlowControlChanged(FlowControl),
    
    // Port list dialog
    ShowPortListDialog,
    HidePortListDialog,
    RefreshPorts,
    SelectPort(String),
    PortsRefreshed(Vec<PortInfo>),
    
    // Configuration panel
    ShowConfigPanel,
    HideConfigPanel,
    CapacityChanged(String),
    TagFileChanged(String),
    LatencyChanged(String),
    ApplyConfig,
    
    // Terminal view
    TerminalInput(String),
    SendCommand,
    ClearTerminal,
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    JumpToStart,
    JumpToEnd,
    
    // Input mode switching
    SwitchToAsciiMode,
    SwitchToHexMode,
    HexInput(String),
    QuickHex(String),
    ClearHexInput,
    
    // Multiplexing protocol mode
    ToggleMuxMode,
    MuxLinkIdChanged(String),
    CopyMuxFrame(String),
    
    // Search
    ToggleSearchMode,
    SearchInput(String),
    SearchNext,
    SearchPrev,
    ToggleCaseSensitive,
    
    // Data operations
    SaveData,
    RecordData,
    CopyToClipboard,
    
    // Plugin commands
    ShowPluginDialog,
    HidePluginDialog,
    LoadPlugin(String),
    UnloadPlugin(String),
    PluginCommand(String, String, Vec<String>),
    
    // Application
    Exit,
    Tick,
    DataReceived(Vec<u8>),
    
    // Menu
    MenuFile,
    MenuSerial,
    MenuHelp,
}

#[derive(Debug, Clone)]
pub struct PortInfo {
    pub name: String,
    pub serial_number: Option<String>,
    pub pid: u16,
    pub vid: u16,
    pub manufacturer: Option<String>,
}

impl From<serialport::SerialPortInfo> for PortInfo {
    fn from(info: serialport::SerialPortInfo) -> Self {
        match info.port_type {
            serialport::SerialPortType::UsbPort(usb_info) => PortInfo {
                name: info.port_name,
                serial_number: usb_info.serial_number,
                pid: usb_info.pid,
                vid: usb_info.vid,
                manufacturer: usb_info.manufacturer,
            },
            _ => PortInfo {
                name: info.port_name,
                serial_number: None,
                pid: 0,
                vid: 0,
                manufacturer: None,
            },
        }
    }
}
