use std::cell::Cell;
use std::cmp::{Eq, PartialEq};
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};
use std::hash::{Hash, Hasher};
use std::mem::MaybeUninit;

#[link(wasm_import_module = "host")]
unsafe extern "C" {
    #[link_name = "create_channel"]
    fn _create_channel(p: *const u8, n: u32, flag: u32) -> u32;
    #[link_name = "publish_message"]
    fn _publish(i: u32, p: *const u8, n: u32);
    #[link_name = "has_message"]
    fn _has_msg(i: u32) -> u32;
    #[link_name = "pop_message"]
    fn _pop_msg(i: u32, p: *mut u8, n: u32) -> u32;
}

const FLAG_PUBLISH: u32 = 1;
const FLAG_SUBSCRIBE: u32 = 2;

#[derive(Clone)]
pub struct ChannelId {
    id: u32,
    flags: Cell<u32>,
}

impl PartialEq for ChannelId {
    fn eq(&self, rhs: &Self) -> bool {
        self.id.eq(&rhs.id)
    }
}

impl Eq for ChannelId {}

impl Hash for ChannelId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Display for ChannelId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "ChannelId")
    }
}

impl Debug for ChannelId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("ChannelId")
            .field("publish", &(self.is_publish()))
            .field("subscribe", &(self.is_subscribe()))
            .finish_non_exhaustive()
    }
}

impl ChannelId {
    pub fn create(key: &[u8], publish: bool, subscribe: bool) -> Self {
        let flags =
            if publish { FLAG_PUBLISH } else { 0 } | if subscribe { FLAG_SUBSCRIBE } else { 0 };
        Self {
            flags: Cell::new(flags),
            id: unsafe { _create_channel(key.as_ptr(), key.len() as _, flags) },
        }
    }

    #[inline(always)]
    pub fn is_publish(&self) -> bool {
        self.flags.get() & FLAG_PUBLISH != 0
    }

    #[inline(always)]
    pub fn is_subscribe(&self) -> bool {
        self.flags.get() & FLAG_SUBSCRIBE != 0
    }

    pub fn merge(&self, other: &Self) {
        self.flags.set(self.flags.get() | other.flags.get());
    }

    pub fn publish(&self, data: &[u8]) {
        if !self.is_publish() {
            panic!("Channel is not publishable!");
        }

        unsafe {
            _publish(self.id, data.as_ptr(), data.len() as _);
        }
    }

    pub fn has_message(&self) -> bool {
        self.is_subscribe() && unsafe { _has_msg(self.id) != 0 }
    }

    pub fn pop_message(&self, buf: &mut [MaybeUninit<u8>]) -> Result<Option<&mut [u8]>, usize> {
        if !self.has_message() {
            return Ok(None);
        }

        let l = unsafe { _pop_msg(self.id, buf.as_mut_ptr() as *mut u8, buf.len() as _) } as usize;
        let Some(b) = buf.get_mut(..l) else {
            return Err(l);
        };
        Ok(Some(unsafe {
            &mut *(b as *mut [MaybeUninit<u8>] as *mut [u8])
        }))
    }
}
