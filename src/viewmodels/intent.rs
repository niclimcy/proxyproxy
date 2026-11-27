use super::mcp_vm::McpClient;
use super::rules_vm::RuleKind;
use crate::domain::config::OnboardingProfile;
use crate::domain::keymap::{Binding, RowAction};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OnboardingTarget {
    Web,
    Ios,
    Android,
}

pub enum OnboardingIntent {
    ChooseProfile(OnboardingProfile),
    ChooseTarget(OnboardingTarget),
    SelectIosSimulator(String),
    Back,
    Device(DeviceAction),
    ImportFlows(PathBuf),
    EnvCopied,
    Finish,
}

pub enum RowIntent {
    Select(u64),
    /// toggle the `app:` filter for this app name
    FilterApp(String),
    Sort(usize),
    ClearFilter,
    Pin(u64),
    Color(u64, Option<u8>),
    Comment(u64),
    Replay(u64),
    Rule(u64, RuleKind),
    Mute(String),
    Scope(String),
    ScopePath(u64),
    /// route only this host through the proxy (replaces the list)
    ProxyOnlyHost(String),
    ProxyOnlyAdd(String),
    Exclude(u64),
    NoTls(String),
    Delete(u64),
    DiffBase(u64),
    TestCommandCopied,
}

pub enum SidebarIntent {
    Pick(String),
    Mute(String),
    Scope(String),
    NoTls(String),
    Unmute(String),
}

#[derive(Clone, Copy)]
pub enum ShortcutIntent {
    ToggleSidebar,
    FocusSearch,
    ClearFilter,
    Step(bool),
    Selected(RowAction),
    Deselect,
    OpenSettings,
    ToggleCapture,
    OpenRowMenu,
}

pub enum KeymapIntent {
    /// Wait for the next Cmd combo for this action.
    Record(RowAction),
    Cancel,
    Set(RowAction, Binding),
    /// Turn the shortcut off.
    Clear(RowAction),
    Reset(RowAction),
    ResetAll,
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

#[derive(Clone, Debug, PartialEq)]
pub enum DeviceAction {
    TrustCa,
    ToggleSystemProxy,
    ToggleProxyOnly,
    LaunchChrome,
    IosSimInstallCa(String),
    InstallMissingIosCas,
    IosSimReboot(String),
    AndroidSetProxy,
    AndroidToggleProxy(String),
    AndroidInstallCa,
}

// Cross-ViewModel follow-ups, routed by the app
pub enum Effect {
    OpenReplay(u64),
    RuleFromFlow(u64, RuleKind),
    ShowOverview,
    DiffBase(u64),
    Device(DeviceAction),
    ImportFlows(PathBuf),
    OpenSettings,
    CloseDetail,
}

pub enum ToolbarIntent {
    Session(SessionIntent),
    Device(DeviceAction),
    Compose,
    About,
    Onboarding,
    Settings,
    ShowBreakpoint,
    ShowScope,
    ClearUnpinned,
    ClearAll,
    ToggleCapture,
    ToggleSidebar,
    ToggleFollow,
    ToggleRules,
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
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ListKind {
    Scope,
    Exclude,
    Muted,
    BypassTls,
    ProxyOnly,
}

#[derive(Clone)]
pub enum RuleIntent {
    SetEnabled(u64, bool),
    Delete(u64),
    /// Swaps with the neighbour in the same collection, so the list order and the priority agree.
    Move(u64, i32),
    SetRuleCollection(u64, Option<u64>),
    SetCollectionEnabled(u64, bool),
    SoloCollection(u64),
    EnableAllCollections,
    /// Creates a collection named after `RulesVm::new_collection`.
    AddCollection,
    RenameCollection(u64, String),
    /// `true` deletes the collection's rules too; `false` moves them to Ungrouped.
    DeleteCollection(u64, bool),
    /// Start a new rule in this collection (and remember it for the next ones).
    NewRuleIn(Option<u64>),
    Edit(u64),
    CancelEdit,
    Submit,
    SetScopeOnly(bool),
    SetProxyOnly(bool),
    SeedProxyOnlyFromScope,
    AddHost(ListKind),
    RemoveHost(ListKind, usize),
    ClearConsole,
    Resolve(bool),
    LoadMapLocalFile,
    SaveMapLocalFile,
}

pub enum NetworkIntent {
    AddDns,
    RemoveDns(usize),
    ImportCa(PathBuf, PathBuf),
    SetKeylog(String),
    SetPac(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum McpCopy {
    Endpoint,
    Client(McpClient),
}

#[derive(Clone, Copy)]
pub enum McpIntent {
    SetEnabled(bool),
    Copied(McpCopy),
}

pub enum SchemasIntent {
    AddFiles(Vec<String>),
    Remove(usize),
    Reload,
}
