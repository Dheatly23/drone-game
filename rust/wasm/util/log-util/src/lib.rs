#![allow(clippy::deref_addrof)]

use std::cell::RefCell;
use std::fmt::{Display, Write};

#[link(wasm_import_module = "host")]
extern "C" {
    #[link_name = "log"]
    fn _log(p: *const u8, n: u32);
}

static mut TEMP: RefCell<String> = RefCell::new(String::new());

pub fn log(v: impl Display) {
    if cfg!(debug_assertions) {
        unsafe {
            let mut s = (&*(&raw const TEMP)).borrow_mut();
            s.clear();
            write!(&mut *s, "{v}").unwrap();
            _log(s.as_ptr(), s.len() as _);
        }
    }
}
