//! Reading string values from the Windows registry: one value by name, or every string value of a
//! key. Absent keys and values, and values of other types, read as nothing.

use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegEnumValueW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_READ, REG_EXPAND_SZ, REG_SZ,
};

/// A root of the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hive {
    CurrentUser,
    LocalMachine,
}

/// A string value: its text, and whether the registry stores it as one that names other variables
/// between percent signs (`REG_EXPAND_SZ`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
    pub text: String,
    pub expandable: bool,
}

/// An open key, closed on drop.
struct Key(HKEY);

impl Key {
    fn open(hive: Hive, subkey: &str) -> Option<Self> {
        let root = match hive {
            Hive::CurrentUser => HKEY_CURRENT_USER,
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
        };
        let subkey = wide(subkey);
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: the name is NUL-terminated and `key` outlives the call.
        let status = unsafe { RegOpenKeyExW(root, subkey.as_ptr(), 0, KEY_READ, &mut key) };
        (status == ERROR_SUCCESS).then_some(Self(key))
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the key was opened by `open` and is closed only here.
        unsafe { RegCloseKey(self.0) };
    }
}

fn wide(text: &str) -> Vec<u16> {
    std::ffi::OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// The string `data` (UTF-16 bytes as the registry returns them) up to its first NUL.
fn text_of(data: &[u16]) -> String {
    let end = data
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(data.len());
    String::from_utf16_lossy(&data[..end])
}

fn value_of(kind: u32, data: &[u16]) -> Option<Value> {
    matches!(kind, REG_SZ | REG_EXPAND_SZ).then(|| Value {
        text: text_of(data),
        expandable: kind == REG_EXPAND_SZ,
    })
}

/// The string value `name` of the key `subkey`.
pub fn read_value(hive: Hive, subkey: &str, name: &str) -> Option<Value> {
    let key = Key::open(hive, subkey)?;
    let name = wide(name);
    let mut units = 256usize;
    loop {
        let mut data = vec![0u16; units];
        let mut size = (units * 2) as u32;
        let mut kind = 0u32;
        // SAFETY: the name is NUL-terminated and the buffer holds `size` bytes, both alive for the
        // call.
        let status = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                std::ptr::null(),
                &mut kind,
                data.as_mut_ptr().cast(),
                &mut size,
            )
        };
        match status {
            ERROR_SUCCESS => return value_of(kind, &data[..(size as usize).div_ceil(2)]),
            ERROR_MORE_DATA => units = (size as usize).div_ceil(2) + 1,
            _ => return None,
        }
    }
}

/// Every string value of the key `subkey`, in the order the registry lists them.
pub fn read_values(hive: Hive, subkey: &str) -> Vec<(String, Value)> {
    let Some(key) = Key::open(hive, subkey) else {
        return Vec::new();
    };
    let mut values = Vec::new();
    let mut index = 0;
    let (mut name_units, mut data_units) = (256usize, 1024usize);
    loop {
        let mut name = vec![0u16; name_units];
        let mut data = vec![0u16; data_units];
        let mut name_length = name_units as u32;
        let mut size = (data_units * 2) as u32;
        let mut kind = 0u32;
        // SAFETY: both buffers hold the lengths passed and outlive the call.
        let status = unsafe {
            RegEnumValueW(
                key.0,
                index,
                name.as_mut_ptr(),
                &mut name_length,
                std::ptr::null(),
                &mut kind,
                data.as_mut_ptr().cast(),
                &mut size,
            )
        };
        match status {
            ERROR_SUCCESS => {
                if let Some(value) = value_of(kind, &data[..(size as usize).div_ceil(2)]) {
                    values.push((
                        String::from_utf16_lossy(&name[..name_length as usize]),
                        value,
                    ));
                }
                index += 1;
            }
            // The index is retried with a buffer that fits the value.
            ERROR_MORE_DATA => {
                name_units = name_units.max(name_length as usize + 1) * 2;
                data_units = data_units.max((size as usize).div_ceil(2) + 1) * 2;
            }
            _ => return values,
        }
    }
}
