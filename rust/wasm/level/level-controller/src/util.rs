use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ptr::write_unaligned;

pub(crate) struct WriteBuf<'a, T> {
    buf: &'a mut [MaybeUninit<u8>],
    n: usize,
    _phantom: PhantomData<T>,
}

impl<'a, T> WriteBuf<'a, T> {
    pub(crate) const fn new(buf: &'a mut [MaybeUninit<u8>]) -> Self {
        Self {
            buf,
            n: 0,
            _phantom: PhantomData,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.n
    }

    pub(crate) fn as_ptr(&self) -> *const T {
        self.buf as *const [MaybeUninit<u8>] as *const T
    }

    pub(crate) fn rest(&mut self) -> &mut [MaybeUninit<u8>] {
        &mut self.buf[self.n * size_of::<T>()..]
    }

    pub(crate) fn push(&mut self, value: T) {
        let size = size_of::<T>();
        let off = self.n * size;
        self.n += 1;

        if size == 0 {
            return;
        }

        unsafe {
            write_unaligned(&raw mut self.buf[off..off + size] as *mut T, value);
        }
    }
}

impl<T> Extend<T> for WriteBuf<'_, T> {
    fn extend<It>(&mut self, it: It)
    where
        It: IntoIterator<Item = T>,
    {
        for v in it {
            self.push(v);
        }
    }
}
