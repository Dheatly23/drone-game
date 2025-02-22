use getrandom::Error;

#[link(wasm_import_module = "host")]
unsafe extern "C" {
    #[link_name = "random"]
    fn _random(p: *mut u8, n: u32);
}

#[unsafe(no_mangle)]
unsafe extern "Rust" fn __getrandom_v03_custom(p: *mut u8, n: usize) -> Result<(), Error> {
    unsafe {
        _random(p, n as _);
    }
    Ok(())
}
