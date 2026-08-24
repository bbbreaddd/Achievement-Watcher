#[cfg(windows)]
use crate::Error;
use crate::Result;

#[cfg(windows)]
const PREFIX: &str = "dpapi:";

#[cfg(windows)]
pub fn protect(value: &str) -> Result<String> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData},
    };
    if value.is_empty() || value.starts_with(PREFIX) {
        return Ok(value.into());
    }
    let mut input = CRYPT_INTEGER_BLOB {
        cbData: value.len() as u32,
        pbData: value.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let success = unsafe {
        CryptProtectData(
            &mut input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if success == 0 {
        return Err(Error::Invalid(
            "Windows could not encrypt the Steam API key".into(),
        ));
    }
    let bytes = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) };
    let encoded: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(format!("{PREFIX}{encoded}"))
}

#[cfg(windows)]
pub fn unprotect(value: &str) -> Result<String> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
        },
    };
    let Some(encoded) = value.strip_prefix(PREFIX) else {
        return Ok(value.into());
    };
    if encoded.len() % 2 != 0 {
        return Err(Error::Invalid(
            "Encrypted Steam API key is malformed".into(),
        ));
    }
    let mut bytes = (0..encoded.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&encoded[index..index + 2], 16))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| Error::Invalid("Encrypted Steam API key is malformed".into()))?;
    let mut input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let success = unsafe {
        CryptUnprotectData(
            &mut input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if success == 0 {
        return Err(Error::Invalid(
            "Windows could not decrypt the Steam API key for this account".into(),
        ));
    }
    let bytes = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) };
    let result = String::from_utf8(bytes.to_vec())
        .map_err(|_| Error::Invalid("Decrypted Steam API key is not UTF-8".into()));
    unsafe {
        LocalFree(output.pbData.cast());
    }
    result
}

#[cfg(not(windows))]
pub fn protect(value: &str) -> Result<String> {
    Ok(value.into())
}

#[cfg(not(windows))]
pub fn unprotect(value: &str) -> Result<String> {
    Ok(value.into())
}
