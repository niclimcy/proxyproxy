use super::rules_vm::RuleKind;
use std::path::PathBuf;

pub enum RowIntent {
    Select(u64),
    Sort(usize),
    Pin(u64),
    Color(u64, Option<u8>),
    Comment(u64),
    Replay(u64),
    Rule(u64, RuleKind),
    Mute(String),
    Scope(String),
    NoTls(String),
    Delete(u64),
    DiffBase(u64),
}

pub enum SidebarIntent {
    Pick(String),
    Mute(String),
    Scope(String),
    NoTls(String),
    Unmute(String),
}

pub enum ShortcutIntent {
    ToggleSidebar,
    FocusSearch,
    ClearFilter,
    Step(bool),
    DeleteSelected,
    ReplaySelected,
    Deselect,
}

pub enum SessionIntent {
    New,
    Open(PathBuf),
    SaveAs(PathBuf),
    ImportFlows(PathBuf),
    ExportHar(PathBuf),
    ExportCsv(PathBuf),
    ExportRules(PathBuf),
    ImportRules(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DeviceAction {
    TrustCa,
    SetSystemProxy,
    ClearSystemProxy,
    LaunchChrome,
    IosSimInstallCa,
    AndroidSetProxy,
    AndroidClearProxy,
    AndroidInstallCa,
}

// Cross-ViewModel follow-ups, routed by the app
pub enum Effect {
    OpenReplay(u64),
    RuleFromFlow(u64, RuleKind),
    ShowOverview,
    DiffBase(u64),
}

pub enum ToolbarIntent {
    Session(SessionIntent),
    Device(DeviceAction),
    Compose,
    ClearUnpinned,
    ClearAll,
    ToggleCapture,
    ToggleToken(&'static str),
    SaveQuery,
    LoadQuery(String),
    DeleteQuery(usize),
    SetColumn(&'static str, bool),
}

pub enum DetailIntent {
    Replay(u64),
    TogglePin(u64),
    SetComment(u64, String),
    DiffAgainst(u64),
    ClearBase,
    SelectWs(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ListKind {
    Scope,
    Muted,
    BypassTls,
}

pub enum RuleIntent {
    SetEnabled(u64, bool),
    Delete(u64),
    Move(usize, i32),
    Edit(u64),
    CancelEdit,
    Submit,
    SetScopeOnly(bool),
    AddHost(ListKind),
    RemoveHost(ListKind, usize),
    ClearConsole,
    Resolve(bool),
}

pub enum NetworkIntent {
    AddDns,
    RemoveDns(usize),
    ImportCa(PathBuf, PathBuf),
    SetKeylog(String),
    SetPac(String),
}

pub enum SchemasIntent {
    AddFiles(Vec<String>),
    Remove(usize),
    Reload,
}
