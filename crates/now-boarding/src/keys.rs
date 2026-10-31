//! Provider keys in the system key store: the Secret Service (GNOME Keyring
//! or KWallet) on Linux, the Credential Manager on Windows, the Keychain on
//! a Mac. They're never written to the settings file, the cache or the log.

use crate::ptv;

const SERVICE: &str = "now-boarding";
const PTV_DEVID: &str = "ptv-devid";
const PTV_KEY: &str = "ptv-key";
const AEROAPI_KEY: &str = "aeroapi-key";

fn entry(name: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, name).map_err(|e| e.to_string())
}

fn read(name: &str) -> Result<Option<String>, String> {
    match entry(name)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// The PTV developer ID and key, if they've been saved. In a debug build,
/// `NOW_BOARDING_PTV_DEVID` and `NOW_BOARDING_PTV_KEY` can stand in, for
/// trying things out where there's no key store; a release build ignores
/// them.
pub fn load_ptv() -> Result<Option<ptv::Keys>, String> {
    #[cfg(debug_assertions)]
    if let (Ok(devid), Ok(key)) = (
        std::env::var("NOW_BOARDING_PTV_DEVID"),
        std::env::var("NOW_BOARDING_PTV_KEY"),
    ) {
        return Ok(Some(ptv::Keys { devid, key }));
    }
    match (read(PTV_DEVID)?, read(PTV_KEY)?) {
        (Some(devid), Some(key)) => Ok(Some(ptv::Keys { devid, key })),
        _ => Ok(None),
    }
}

pub fn save_ptv(k: &ptv::Keys) -> Result<(), String> {
    entry(PTV_DEVID)?
        .set_password(k.devid.trim())
        .map_err(|e| e.to_string())?;
    entry(PTV_KEY)?
        .set_password(k.key.trim())
        .map_err(|e| e.to_string())
}

/// The AeroAPI key, if it's been saved. In a debug build,
/// `NOW_BOARDING_AEROAPI_KEY` can stand in; a release build ignores it.
pub fn load_aeroapi() -> Result<Option<String>, String> {
    #[cfg(debug_assertions)]
    if let Ok(key) = std::env::var("NOW_BOARDING_AEROAPI_KEY") {
        return Ok(Some(key));
    }
    read(AEROAPI_KEY)
}

pub fn save_aeroapi(key: &str) -> Result<(), String> {
    entry(AEROAPI_KEY)?
        .set_password(key.trim())
        .map_err(|e| e.to_string())
}

pub fn forget_aeroapi() -> Result<(), String> {
    forget(&[AEROAPI_KEY])
}

pub fn forget_ptv() -> Result<(), String> {
    forget(&[PTV_DEVID, PTV_KEY])
}

fn forget(names: &[&str]) -> Result<(), String> {
    for &name in names {
        match entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}
