use core::fmt::{Arguments, Write};

#[link(wasm_import_module = "host")]
extern "C" {
    #[link_name = "log"]
    fn _log(p: *const u8, n: usize);
}

pub fn log(s: &str) {
    // Wraps the safety
    unsafe { _log(s.as_ptr(), s.len()) }
}

static mut TEMP_STR: String = String::new();

pub fn print_log(args: Arguments) {
    // Wraps the safety
    unsafe {
        TEMP_STR.clear();
        TEMP_STR.write_fmt(args).unwrap();
        log(&TEMP_STR);
    }
}
