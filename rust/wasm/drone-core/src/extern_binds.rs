use core::cell::RefCell;
use core::fmt::{Arguments, Write};

#[link(wasm_import_module = "host")]
extern "C" {
    #[link_name = "log"]
    fn _log(p: *const u8, n: usize);
    #[link_name = "get_config_length"]
    fn _get_config_length() -> usize;
    #[link_name = "get_config"]
    fn _get_config(p: *mut u8);
    #[link_name = "read_key_msg"]
    fn _read_key_msg(kp: *mut u8, mp: *mut u8);
    #[link_name = "pubsub_get"]
    fn _pubsub_get();
    #[link_name = "pubsub_listen"]
    fn _pubsub_listen(p: *const u8, l: usize);
    #[link_name = "pubsub_publish"]
    fn _pubsub_publish(kp: *const u8, kl: usize, mp: *const u8, ml: usize);
}

pub fn log(s: &str) {
    // SAFETY: Wraps extern call
    unsafe { _log(s.as_ptr(), s.len()) }
}

static mut TEMP_CFG: Option<Vec<u8>> = None;

pub fn get_config() -> &'static [u8] {
    // SAFETY: Wraps extern call
    unsafe {
        loop {
            if let Some(ret) = &TEMP_CFG {
                return &ret;
            }

            let l = _get_config_length();
            let mut v = vec![0; l];
            if l > 0 {
                _get_config(v.as_mut_ptr());
            }
            TEMP_CFG = Some(v);
        }
    }
}

static mut TEMP_STR: RefCell<String> = RefCell::new(String::new());

pub fn print_log(args: Arguments) {
    // SAFETY: Wraps static mut
    let mut guard = unsafe { TEMP_STR.borrow_mut() };
    guard.clear();
    guard.write_fmt(args).unwrap();
    log(&guard);
}

static mut TEMP_MSG: Option<(Vec<u8>, Vec<u8>)> = None;

#[no_mangle]
extern "C" fn read_msg(klen: usize, mlen: usize) {
    // SAFETY: Wraps extern call
    unsafe {
        let mut kv = vec![0; klen];
        let mut mv = vec![0; mlen];
        _read_key_msg(kv.as_mut_ptr(), mv.as_mut_ptr());
        TEMP_MSG = Some((kv, mv));
    }
}

pub fn pubsub_get() -> Option<(Vec<u8>, Vec<u8>)> {
    // SAFETY: Wraps extern call
    unsafe {
        _pubsub_get();
        TEMP_MSG.take()
    }
}

pub fn pubsub_listen(key: &[u8]) {
    // SAFETY: Wraps extern call
    unsafe { _pubsub_listen(key.as_ptr(), key.len()) }
}

pub fn pubsub_publish(key: &[u8], msg: &[u8]) {
    // SAFETY: Wraps extern call
    unsafe { _pubsub_publish(key.as_ptr(), key.len(), msg.as_ptr(), msg.len()) }
}
