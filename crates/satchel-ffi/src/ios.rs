//! C entry points for the iOS app. The JNI symbols stay on Android.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;
use std::ptr;

use crate::{
    CatalogFailure, assemble_grant_json, catalog_grant, join_pairing, maker, mark_grant,
    mark_sample, parse_invite, read_grant_bytes, read_sample, sample_catalog,
};
use satchel_core::DeviceId;

fn text_from(pointer: *const c_char) -> Result<String, CatalogFailure> {
    if pointer.is_null() {
        return Err(CatalogFailure);
    }
    let value = unsafe { CStr::from_ptr(pointer) };
    let text = value.to_str().map_err(|_| CatalogFailure)?.to_string();
    if text.is_empty() {
        return Err(CatalogFailure);
    }
    Ok(text)
}

/// Device label. Empty means this phone has not chosen a name yet.
fn optional_text_from(pointer: *const c_char) -> Result<String, CatalogFailure> {
    if pointer.is_null() {
        return Err(CatalogFailure);
    }
    let value = unsafe { CStr::from_ptr(pointer) };
    let text = value.to_str().map_err(|_| CatalogFailure)?;
    Ok(text.to_string())
}

fn leak_string(text: String) -> *mut c_char {
    match CString::new(text) {
        Ok(value) => value.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

fn leak_bytes(bytes: Vec<u8>, len: *mut usize) -> *mut u8 {
    if len.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        *len = bytes.len();
    }
    let mut boxed = bytes.into_boxed_slice();
    let pointer = boxed.as_mut_ptr();
    std::mem::forget(boxed);
    pointer
}

/// Release a string returned by this library.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_free_string(value: *mut c_char) {
    if value.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(value));
    }
}

/// Release bytes returned by this library.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_free_bytes(value: *mut u8, len: usize) {
    if value.is_null() {
        return;
    }
    unsafe {
        drop(Vec::from_raw_parts(value, len, len));
    }
}

/// Invite JSON from a scanned QR payload. NULL when the payload is rejected.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_parse_invite(text: *const c_char) -> *mut c_char {
    let Ok(raw) = text_from(text) else {
        return ptr::null_mut();
    };
    match parse_invite(&raw) {
        Ok(json) => leak_string(json),
        Err(_) => ptr::null_mut(),
    }
}

/// Opaque device id. The caller frees the string.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_new_device_id() -> *mut c_char {
    leak_string(DeviceId::generate().as_str().to_string())
}

/// Grant JSON with the device id and work path added. NULL on rejection.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_assemble_grant(
    joined: *const c_char,
    device_id: *const c_char,
    work: *const c_char,
) -> *mut c_char {
    let Ok(joined_text) = text_from(joined) else {
        return ptr::null_mut();
    };
    let Ok(id_text) = text_from(device_id) else {
        return ptr::null_mut();
    };
    let Ok(work_text) = text_from(work) else {
        return ptr::null_mut();
    };
    match assemble_grant_json(&joined_text, &id_text, &work_text) {
        Ok(json) => leak_string(json),
        Err(_) => ptr::null_mut(),
    }
}

/// Pairing grant before the device id is added. NULL when the desktop denies or the fields are wrong.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_join_pairing(
    address: *const c_char,
    code: *const c_char,
    fingerprint: *const c_char,
    device_name: *const c_char,
    os_name: *const c_char,
    device_id: *const c_char,
) -> *mut c_char {
    let Ok(address_text) = text_from(address) else {
        return ptr::null_mut();
    };
    let Ok(code_text) = text_from(code) else {
        return ptr::null_mut();
    };
    let Ok(fingerprint_text) = text_from(fingerprint) else {
        return ptr::null_mut();
    };
    let Ok(name_text) = text_from(device_name) else {
        return ptr::null_mut();
    };
    let Ok(os_text) = text_from(os_name) else {
        return ptr::null_mut();
    };
    let Ok(id_text) = text_from(device_id) else {
        return ptr::null_mut();
    };
    match join_pairing(
        &address_text,
        &code_text,
        &fingerprint_text,
        &name_text,
        &os_text,
        &id_text,
        satchel_pair::DEFAULT_TIMEOUT,
    ) {
        Ok(json) => leak_string(json),
        Err(_) => ptr::null_mut(),
    }
}

/// Sample catalog metadata. The JSON has no file bytes.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_sample_catalog(root: *const c_char) -> *mut c_char {
    let Ok(root_text) = text_from(root) else {
        return ptr::null_mut();
    };
    match sample_catalog(Path::new(&root_text)) {
        Ok(json) => leak_string(json),
        Err(_) => ptr::null_mut(),
    }
}

/// Write a follow mark into the sample repository. Returns 0 on success.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_mark_sample(
    root: *const c_char,
    path: *const c_char,
    follow: *const c_char,
    name: *const c_char,
    os: *const c_char,
) -> i32 {
    let Ok(root_text) = text_from(root) else {
        return 1;
    };
    let Ok(path_text) = text_from(path) else {
        return 1;
    };
    let Ok(follow_text) = text_from(follow) else {
        return 1;
    };
    let Ok(name_text) = optional_text_from(name) else {
        return 1;
    };
    let Ok(os_text) = optional_text_from(os) else {
        return 1;
    };
    match mark_sample(
        Path::new(&root_text),
        &path_text,
        &follow_text,
        &name_text,
        &os_text,
    ) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// File bytes from the sample repository. Sets `len` and returns NULL on failure.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_read_sample(
    root: *const c_char,
    path: *const c_char,
    len: *mut usize,
) -> *mut u8 {
    if !len.is_null() {
        unsafe {
            *len = 0;
        }
    }
    let Ok(root_text) = text_from(root) else {
        return ptr::null_mut();
    };
    let Ok(path_text) = text_from(path) else {
        return ptr::null_mut();
    };
    match read_sample(Path::new(&root_text), &path_text) {
        Ok(bytes) => leak_bytes(bytes, len),
        Err(_) => ptr::null_mut(),
    }
}

/// Catalog opened from a pairing grant.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_catalog_grant(spec_path: *const c_char) -> *mut c_char {
    let Ok(spec_text) = text_from(spec_path) else {
        return ptr::null_mut();
    };
    match catalog_grant(Path::new(&spec_text)) {
        Ok(json) => leak_string(json),
        Err(_) => ptr::null_mut(),
    }
}

/// Write a follow mark through a pairing grant. Returns 0 on success.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_mark_grant(
    spec_path: *const c_char,
    path: *const c_char,
    follow: *const c_char,
    name: *const c_char,
    os: *const c_char,
) -> i32 {
    let Ok(spec_text) = text_from(spec_path) else {
        return 1;
    };
    let Ok(path_text) = text_from(path) else {
        return 1;
    };
    let Ok(follow_text) = text_from(follow) else {
        return 1;
    };
    let Ok(name_text) = optional_text_from(name) else {
        return 1;
    };
    let Ok(os_text) = optional_text_from(os) else {
        return 1;
    };
    match mark_grant(
        Path::new(&spec_text),
        &path_text,
        &follow_text,
        &name_text,
        &os_text,
    ) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// File bytes through a pairing grant.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_read_grant(
    spec_path: *const c_char,
    path: *const c_char,
    len: *mut usize,
) -> *mut u8 {
    if !len.is_null() {
        unsafe {
            *len = 0;
        }
    }
    let Ok(spec_text) = text_from(spec_path) else {
        return ptr::null_mut();
    };
    let Ok(path_text) = text_from(path) else {
        return ptr::null_mut();
    };
    match read_grant_bytes(Path::new(&spec_text), &path_text) {
        Ok(bytes) => leak_bytes(bytes, len),
        Err(_) => ptr::null_mut(),
    }
}

/// Create the repository. Returns `password\ngrant JSON`, or NULL if refused. The caller keeps
/// the grant in the Keychain; nothing is written to disk here.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_create_repository(
    spec: *const c_char,
    work: *const c_char,
) -> *mut c_char {
    let (Ok(spec), Ok(work)) = (text_from(spec), text_from(work)) else {
        return ptr::null_mut();
    };
    match maker::create_phone_repository(&spec, Path::new(&work)) {
        Ok(created) => leak_string(created),
        Err(_) => ptr::null_mut(),
    }
}

/// Why the last create was refused: `protection`, `exists`, `unreachable`, or `fields`.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_create_failure() -> *mut c_char {
    leak_string(maker::last_failure())
}

/// 1 when the grant file belongs to the device that created the repository.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_is_maintainer(grant: *const c_char) -> i32 {
    let Ok(grant) = text_from(grant) else {
        return 0;
    };
    i32::from(maker::is_maintainer(Path::new(&grant)))
}

/// Start showing the invite. `bind` may be empty. Offer JSON, or NULL.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_hold_start(grant: *const c_char, bind: *const c_char) -> *mut c_char {
    let (Ok(grant), Ok(bind)) = (text_from(grant), optional_text_from(bind)) else {
        return ptr::null_mut();
    };
    match maker::hold_start(Path::new(&grant), &bind, satchel_pair::DEFAULT_TIMEOUT) {
        Ok(offer) => leak_string(offer),
        Err(_) => ptr::null_mut(),
    }
}

/// Approval JSON once the other device passed the code check, an empty string before, NULL on error.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_hold_poll() -> *mut c_char {
    match maker::hold_poll() {
        Ok(shown) => leak_string(shown),
        Err(_) => ptr::null_mut(),
    }
}

/// 1 when the decision was applied. Approving writes the other device's key.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_hold_decide(grant: *const c_char, approve: i32) -> i32 {
    let Ok(grant) = text_from(grant) else {
        return 0;
    };
    i32::from(maker::hold_decide(Path::new(&grant), approve != 0).is_ok())
}

/// Stop listening without a decision.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_hold_close() {
    maker::hold_close();
}

/// PNG bytes of the invite QR. NULL on error. Free with `satchel_free_bytes`.
#[unsafe(no_mangle)]
pub extern "C" fn satchel_invite_png(invite: *const c_char, len: *mut usize) -> *mut u8 {
    let Ok(invite) = text_from(invite) else {
        return ptr::null_mut();
    };
    match maker::invite_png(&invite) {
        Ok(bytes) => leak_bytes(bytes, len),
        Err(_) => ptr::null_mut(),
    }
}
