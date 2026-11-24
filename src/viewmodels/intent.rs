use super::rules_vm::RuleKind;
use crate::domain::config::OnboardingProfile;
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

pub enum ShortcutIntent {
    ToggleSidebar,
    FocusSearch,
    ClearFilter,
    Step(bool),
    DeleteSelected,
    ReplaySelected,
    Deselect,
    OpenSettings,
    ToggleCapture,
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
    Exclude,
    Muted,
    BypassTls,
    ProxyOnly,
}

pub enum RuleIntent {
    SetEnabled(u64, bool),
    Delete(u64),
    Move(usize, i32),
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

pub enum SchemasIntent {
    AddFiles(Vec<String>),
    Remove(usize),
    Reload,
}
