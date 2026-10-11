use anyhow::{anyhow, Result};
use std::fs;
use std::path::Path;
use zeroize::Zeroize;

#[cfg(windows)]
use windows::Win32::Foundation::{LocalFree, HLOCAL};
#[cfg(windows)]
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_LOCAL_MACHINE, CRYPT_INTEGER_BLOB,
};

pub fn protect(plain: &[u8]) -> Result<Vec<u8>> {
    #[cfg(windows)]
    {
        return dpapi_protect(plain);
    }
    #[cfg(not(windows))]
    {
        Ok(plain.to_vec())
    }
}

pub fn unprotect(blob: &[u8]) -> Result<Vec<u8>> {
    #[cfg(windows)]
    {
        return dpapi_unprotect(blob);
    }
    #[cfg(not(windows))]
    {
        Ok(blob.to_vec())
    }
}

pub fn protect_to_file(path: &Path, plain: &[u8]) -> Result<()> {
    let blob = protect(plain)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        crate::paths::apply_admin_acl(parent)?;
    }
    fs::write(path, blob)?;
    crate::paths::apply_admin_acl(path)?;
    Ok(())
}

pub fn unprotect_from_file(path: &Path) -> Result<Vec<u8>> {
    let blob = fs::read(path)?;
    unprotect(&blob)
}

#[cfg(windows)]
fn dpapi_protect(plain: &[u8]) -> Result<Vec<u8>> {
    let mut input = CRYPT_INTEGER_BLOB {
        cbData: plain.len() as u32,
        pbData: plain.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &mut input,
            windows::core::PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_LOCAL_MACHINE,
            &mut output,
        )
        .map_err(|e| anyhow!("CryptProtectData: {e}"))?;
        let slice = std::slice::from_raw_parts(output.pbData, output.cbData as usize);
        let out = slice.to_vec();
        let _ = LocalFree(HLOCAL(output.pbData as *mut _));
        Ok(out)
    }
}

#[cfg(windows)]
fn dpapi_unprotect(blob: &[u8]) -> Result<Vec<u8>> {
    let mut input = CRYPT_INTEGER_BLOB {
        cbData: blob.len() as u32,
        pbData: blob.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(&mut input, None, None, None, None, 0, &mut output)
            .map_err(|e| anyhow!("CryptUnprotectData: {e}"))?;
        let slice = std::slice::from_raw_parts(output.pbData, output.cbData as usize);
        let out = slice.to_vec();
        let _ = LocalFree(HLOCAL(output.pbData as *mut _));
        Ok(out)
    }
}

pub fn zeroize_vec(v: &mut Vec<u8>) {
    v.zeroize();
}
