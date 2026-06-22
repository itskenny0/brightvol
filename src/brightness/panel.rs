//! Windows internal-panel brightness via WMI (`root\WMI`).
//!
//! Reads `WmiMonitorBrightness.CurrentBrightness` and sets brightness through
//! `WmiMonitorBrightnessMethods.WmiSetBrightness`.
//!
//! COM must already be initialized on the calling thread (see `main`). All
//! calls happen on that single thread, so the non-`Send` COM interfaces never
//! cross threads.

use super::{calc, BrightnessError, Result};
use windows::core::{w, BSTR, PCWSTR, VARIANT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoSetProxyBlanket, CLSCTX_INPROC_SERVER, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL,
    RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE};
use windows::Win32::System::Wmi::{
    IWbemClassObject, IWbemLocator, IWbemServices, WbemLocator, WBEM_FLAG_FORWARD_ONLY,
    WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_GENERIC_FLAG_TYPE, WBEM_INFINITE,
};

/// Live connection to the WMI brightness provider.
pub struct Brightness {
    services: IWbemServices,
}

impl Brightness {
    /// Connect to `root\WMI`. Requires COM to be initialized on this thread.
    pub fn connect() -> Result<Self> {
        unsafe {
            let locator: IWbemLocator =
                CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER).map_err(win)?;

            let services: IWbemServices = locator
                .ConnectServer(
                    &BSTR::from("ROOT\\WMI"),
                    &BSTR::new(),
                    &BSTR::new(),
                    &BSTR::new(),
                    0,
                    &BSTR::new(),
                    None,
                )
                .map_err(win)?;

            CoSetProxyBlanket(
                &services,
                RPC_C_AUTHN_WINNT,
                RPC_C_AUTHZ_NONE,
                PCWSTR::null(),
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
            )
            .map_err(win)?;

            Ok(Brightness { services })
        }
    }

    /// Current brightness as a percentage (0..=100).
    pub fn current(&self) -> Result<u8> {
        unsafe {
            let obj = self
                .first_instance("SELECT * FROM WmiMonitorBrightness")?
                .ok_or_else(|| BrightnessError("no WmiMonitorBrightness instance".into()))?;
            let mut var = VARIANT::new();
            obj.Get(w!("CurrentBrightness"), 0, &mut var, None, None)
                .map_err(win)?;
            // CurrentBrightness is a uint8; read it through the coercing reader.
            let value = u32::try_from(&var).map_err(win)?;
            Ok(value.min(100) as u8)
        }
    }

    /// Set brightness to `level` (clamped to 0..=100).
    pub fn set(&self, level: u8) -> Result<()> {
        let level = level.min(100);
        unsafe {
            // Path of an actual WmiMonitorBrightnessMethods instance.
            let inst = self
                .first_instance("SELECT * FROM WmiMonitorBrightnessMethods")?
                .ok_or_else(|| BrightnessError("no WmiMonitorBrightnessMethods instance".into()))?;
            let mut path_var = VARIANT::new();
            inst.Get(w!("__PATH"), 0, &mut path_var, None, None)
                .map_err(win)?;
            let path = BSTR::try_from(&path_var).map_err(win)?;

            // Build the WmiSetBrightness input parameters.
            let mut class_obj: Option<IWbemClassObject> = None;
            self.services
                .GetObject(
                    &BSTR::from("WmiMonitorBrightnessMethods"),
                    WBEM_GENERIC_FLAG_TYPE(0),
                    None,
                    Some(&mut class_obj),
                    None,
                )
                .map_err(win)?;
            let class_obj =
                class_obj.ok_or_else(|| BrightnessError("failed to get method class".into()))?;

            let mut in_sig: Option<IWbemClassObject> = None;
            let mut out_sig: Option<IWbemClassObject> = None;
            class_obj
                .GetMethod(w!("WmiSetBrightness"), 0, &mut in_sig, &mut out_sig)
                .map_err(win)?;
            let in_sig =
                in_sig.ok_or_else(|| BrightnessError("WmiSetBrightness has no in-params".into()))?;
            let in_inst = in_sig.SpawnInstance(0).map_err(win)?;

            // Timeout is uint32, Brightness is uint8 in the WMI method signature.
            let timeout = VARIANT::from(0u32);
            in_inst.Put(w!("Timeout"), 0, &timeout, 0).map_err(win)?;
            let brightness = VARIANT::from(level);
            in_inst
                .Put(w!("Brightness"), 0, &brightness, 0)
                .map_err(win)?;

            self.services
                .ExecMethod(
                    &path,
                    &BSTR::from("WmiSetBrightness"),
                    WBEM_GENERIC_FLAG_TYPE(0),
                    None,
                    &in_inst,
                    None,
                    None,
                )
                .map_err(win)?;
        }
        Ok(())
    }

    /// Step brightness by `delta` percentage points and return the new level.
    pub fn step(&self, delta: i8) -> Result<u8> {
        let current = self.current()?;
        let next = calc::step_level(current, delta, &[]);
        self.set(next)?;
        Ok(next)
    }

    /// Run a WQL query and return its first instance, if any.
    unsafe fn first_instance(&self, query: &str) -> Result<Option<IWbemClassObject>> {
        let enumerator = self
            .services
            .ExecQuery(
                &BSTR::from("WQL"),
                &BSTR::from(query),
                WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                None,
            )
            .map_err(win)?;

        let mut objs: [Option<IWbemClassObject>; 1] = [None];
        let mut returned: u32 = 0;
        // Returns a non-fatal HRESULT (e.g. WBEM_S_FALSE) when the enumeration
        // is exhausted; we just check how many instances came back.
        let _ = enumerator.Next(WBEM_INFINITE, &mut objs, &mut returned);
        if returned == 0 {
            return Ok(None);
        }
        Ok(objs[0].take())
    }
}

/// Map a Windows COM error into our error type.
fn win(e: windows::core::Error) -> BrightnessError {
    BrightnessError(e.message())
}
