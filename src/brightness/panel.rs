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
            let locator: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER)
                .map_err(err("CoCreateInstance"))?;

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
                .map_err(err("ConnectServer"))?;

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
            .map_err(err("CoSetProxyBlanket"))?;

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
                .map_err(err("Get CurrentBrightness"))?;
            // CurrentBrightness is a uint8; read it through the coercing reader.
            let value = u32::try_from(&var).map_err(err("read CurrentBrightness"))?;
            Ok(value.min(100) as u8)
        }
    }

    /// Set brightness to `level` (clamped to 0..=100).
    pub fn set(&self, level: u8) -> Result<()> {
        let level = level.min(100);
        unsafe {
            // The relative path of an actual WmiMonitorBrightnessMethods
            // instance. ExecMethod on a namespace-connected IWbemServices wants
            // the relative path (`__RELPATH`), not the full `__PATH`.
            let inst = self
                .first_instance("SELECT * FROM WmiMonitorBrightnessMethods")?
                .ok_or_else(|| BrightnessError("no WmiMonitorBrightnessMethods instance".into()))?;
            let mut path_var = VARIANT::new();
            inst.Get(w!("__RELPATH"), 0, &mut path_var, None, None)
                .map_err(err("Get __RELPATH"))?;
            let path = BSTR::try_from(&path_var).map_err(err("read __RELPATH"))?;

            // Build the WmiSetBrightness input parameters from the method's
            // in-parameter signature.
            let mut class_obj: Option<IWbemClassObject> = None;
            self.services
                .GetObject(
                    &BSTR::from("WmiMonitorBrightnessMethods"),
                    WBEM_GENERIC_FLAG_TYPE(0),
                    None,
                    Some(&mut class_obj),
                    None,
                )
                .map_err(err("GetObject"))?;
            let class_obj =
                class_obj.ok_or_else(|| BrightnessError("failed to get method class".into()))?;

            let mut in_sig: Option<IWbemClassObject> = None;
            let mut out_sig: Option<IWbemClassObject> = None;
            class_obj
                .GetMethod(w!("WmiSetBrightness"), 0, &mut in_sig, &mut out_sig)
                .map_err(err("GetMethod"))?;
            let in_sig = in_sig
                .ok_or_else(|| BrightnessError("WmiSetBrightness has no in-params".into()))?;
            let in_inst = in_sig.SpawnInstance(0).map_err(err("SpawnInstance"))?;

            // WMI represents integer property values as VT_I4 regardless of the
            // declared CIM type (uint32/uint8). Passing VT_UI4/VT_UI1 here is
            // rejected with WBEM_E_TYPE_MISMATCH (0x80041005), so use i32.
            let timeout = VARIANT::from(1i32);
            in_inst
                .Put(w!("Timeout"), 0, &timeout, 0)
                .map_err(err("Put Timeout"))?;
            let brightness = VARIANT::from(level as i32);
            in_inst
                .Put(w!("Brightness"), 0, &brightness, 0)
                .map_err(err("Put Brightness"))?;

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
                .map_err(err("ExecMethod"))?;
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
            .map_err(err("ExecQuery"))?;

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

/// Map a Windows COM error into our error type, tagging which call failed and
/// the HRESULT code (WBEM error codes have no system message text, so the code
/// is what identifies them).
fn err(context: &'static str) -> impl Fn(windows::core::Error) -> BrightnessError {
    move |e| {
        let msg = e.message();
        if msg.is_empty() {
            BrightnessError(format!("{context}: hr=0x{:08X}", e.code().0 as u32))
        } else {
            BrightnessError(format!("{context}: hr=0x{:08X} {msg}", e.code().0 as u32))
        }
    }
}
