use super::Res;

const HINT: &str = "system proxy isn't supported on this OS; use the env snippet or set it manually";

pub fn system_proxy(_: bool, _: u16) -> Res {
    Err(HINT.into())
}

pub fn system_proxy_status(_: u16) -> Result<bool, String> {
    Err(HINT.into())
}

pub fn pac_status() -> Result<bool, String> {
    Err(HINT.into())
}

pub fn set_pac(_: &str, _: u16) -> Result<(), String> {
    Err(HINT.into())
}
