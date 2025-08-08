#![allow(non_camel_case_types)]
use std::ffi::{c_char, c_long, c_uint, c_void, CStr};
use std::sync::{Mutex, OnceLock};

#[cfg(target_os = "windows")]
use libloading::{Library, Symbol};

#[cfg(not(target_os = "windows"))]
mod non_windows_stubs {
    use super::*;
    #[no_mangle]
    pub extern "C" fn set_libcurl_path(_path: *const c_char) -> bool { false }
    #[no_mangle]
    pub extern "C" fn curl_version() -> *const c_char { std::ptr::null() }
    #[no_mangle]
    pub extern "C" fn curl_global_init(_flags: c_long) -> c_uint { 1 }
    #[no_mangle]
    pub extern "C" fn curl_global_cleanup() {}
    #[no_mangle]
    pub extern "C" fn curl_easy_init() -> *mut c_void { std::ptr::null_mut() }
    #[no_mangle]
    pub extern "C" fn curl_easy_cleanup(_handle: *mut c_void) {}
    #[no_mangle]
    pub extern "C" fn curl_easy_perform(_handle: *mut c_void) -> c_uint { 1 }
    #[no_mangle]
    pub extern "C" fn curl_easy_strerror(_code: c_uint) -> *const c_char { std::ptr::null() }
}

#[cfg(target_os = "windows")]
mod win {
    use super::*;

    type CURL = c_void;
    type CURLcode = c_uint;

    struct LibFns {
        curl_version: unsafe extern "C" fn() -> *const c_char,
        curl_global_init: unsafe extern "C" fn(c_long) -> CURLcode,
        curl_global_cleanup: unsafe extern "C" fn(),
        curl_easy_init: unsafe extern "C" fn() -> *mut CURL,
        curl_easy_cleanup: unsafe extern "C" fn(*mut CURL),
        curl_easy_perform: unsafe extern "C" fn(*mut CURL) -> CURLcode,
        curl_easy_strerror: unsafe extern "C" fn(CURLcode) -> *const c_char,
    }

    struct LibState {
        dll_path_utf8: Option<String>,
        lib: Option<Library>,
        fns: Option<LibFns>,
    }

    impl LibState {
        fn new() -> Self { Self { dll_path_utf8: None, lib: None, fns: None } }
    }

    static STATE: OnceLock<Mutex<LibState>> = OnceLock::new();

    fn with_state<T>(f: impl FnOnce(&mut LibState) -> T) -> T {
        let lock = STATE.get_or_init(|| Mutex::new(LibState::new()));
        let mut guard = lock.lock().unwrap();
        f(&mut *guard)
    }

    fn ensure_loaded(state: &mut LibState) -> Result<(), ()> {
        if state.fns.is_some() { return Ok(()); }
        let path = match &state.dll_path_utf8 {
            Some(p) => p.clone(),
            None => return Err(()),
        };
        unsafe {
            let lib = Library::new(&path).map_err(|_| ())?;
            let curl_version: Symbol<unsafe extern "C" fn() -> *const c_char> = lib.get(b"curl_version\0").map_err(|_| ())?;
            let curl_global_init: Symbol<unsafe extern "C" fn(c_long) -> CURLcode> = lib.get(b"curl_global_init\0").map_err(|_| ())?;
            let curl_global_cleanup: Symbol<unsafe extern "C" fn()> = lib.get(b"curl_global_cleanup\0").map_err(|_| ())?;
            let curl_easy_init: Symbol<unsafe extern "C" fn() -> *mut CURL> = lib.get(b"curl_easy_init\0").map_err(|_| ())?;
            let curl_easy_cleanup: Symbol<unsafe extern "C" fn(*mut CURL)> = lib.get(b"curl_easy_cleanup\0").map_err(|_| ())?;
            let curl_easy_perform: Symbol<unsafe extern "C" fn(*mut CURL) -> CURLcode> = lib.get(b"curl_easy_perform\0").map_err(|_| ())?;
            let curl_easy_strerror: Symbol<unsafe extern "C" fn(CURLcode) -> *const c_char> = lib.get(b"curl_easy_strerror\0").map_err(|_| ())?;

            state.fns = Some(LibFns {
                curl_version: *curl_version,
                curl_global_init: *curl_global_init,
                curl_global_cleanup: *curl_global_cleanup,
                curl_easy_init: *curl_easy_init,
                curl_easy_cleanup: *curl_easy_cleanup,
                curl_easy_perform: *curl_easy_perform,
                curl_easy_strerror: *curl_easy_strerror,
            });
            state.lib = Some(lib);
        }
        Ok(())
    }

    #[no_mangle]
    pub extern "C" fn set_libcurl_path(path: *const c_char) -> bool {
        if path.is_null() { return false; }
        let c_str = unsafe { CStr::from_ptr(path) };
        match c_str.to_str() {
            Ok(s) => with_state(|st| { st.dll_path_utf8 = Some(s.to_string()); st.lib = None; st.fns = None; true }),
            Err(_) => false,
        }
    }

    #[no_mangle]
    pub extern "C" fn curl_version() -> *const c_char {
        with_state(|st| {
            if ensure_loaded(st).is_err() { return std::ptr::null(); }
            unsafe { (st.fns.as_ref().unwrap().curl_version)() }
        })
    }

    #[no_mangle]
    pub extern "C" fn curl_global_init(flags: c_long) -> c_uint {
        with_state(|st| {
            if ensure_loaded(st).is_err() { return 1; }
            unsafe { (st.fns.as_ref().unwrap().curl_global_init)(flags) }
        })
    }

    #[no_mangle]
    pub extern "C" fn curl_global_cleanup() {
        with_state(|st| {
            if ensure_loaded(st).is_err() { return; }
            unsafe { (st.fns.as_ref().unwrap().curl_global_cleanup)() }
        })
    }

    #[no_mangle]
    pub extern "C" fn curl_easy_init() -> *mut c_void {
        with_state(|st| {
            if ensure_loaded(st).is_err() { return std::ptr::null_mut(); }
            unsafe { (st.fns.as_ref().unwrap().curl_easy_init)() as *mut c_void }
        })
    }

    #[no_mangle]
    pub extern "C" fn curl_easy_cleanup(handle: *mut c_void) {
        with_state(|st| {
            if ensure_loaded(st).is_err() { return; }
            unsafe { (st.fns.as_ref().unwrap().curl_easy_cleanup)(handle as *mut _); }
        })
    }

    #[no_mangle]
    pub extern "C" fn curl_easy_perform(handle: *mut c_void) -> c_uint {
        with_state(|st| {
            if ensure_loaded(st).is_err() { return 1; }
            unsafe { (st.fns.as_ref().unwrap().curl_easy_perform)(handle as *mut _) }
        })
    }

    #[no_mangle]
    pub extern "C" fn curl_easy_strerror(code: c_uint) -> *const c_char {
        with_state(|st| {
            if ensure_loaded(st).is_err() { return std::ptr::null(); }
            unsafe { (st.fns.as_ref().unwrap().curl_easy_strerror)(code) }
        })
    }
}
