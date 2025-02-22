use std::mem::MaybeUninit;

#[link(wasm_import_module = "host")]
unsafe extern "C" {
    #[link_name = "read_data"]
    pub fn read_data(p: *mut u8, n: u64) -> u64;
    #[link_name = "write_data"]
    pub fn write_data(p: *const u8, n: u64);
}

const BUF_LEN: usize = if cfg!(feature = "buffer-large") {
    64 * 1024 * 1024
} else {
    1024 * 1024
};
#[repr(C, align(65536))]
struct StaticBuffer([u8; BUF_LEN]);
static mut BUFFER: StaticBuffer = StaticBuffer([0; BUF_LEN]);

pub unsafe fn read() -> &'static [u8] {
    unsafe {
        let i = read_data(&raw mut BUFFER.0 as _, BUF_LEN as _);
        &*(&raw const BUFFER.0[..i as usize])
    }
}

pub unsafe fn write(f: impl FnOnce(&mut [MaybeUninit<u8>]) -> usize) {
    unsafe {
        let l = f(&mut *((&raw mut BUFFER.0) as *mut [u8] as *mut [MaybeUninit<u8>]));
        write_data((&raw const BUFFER.0) as _, l as _);
    }
}

pub unsafe fn buffer<'a>() -> &'a mut [u8] {
    unsafe { &mut *(&raw mut BUFFER.0[..]) }
}
