// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use core::num::NonZeroU8;
use core::ops::Deref;
use core::pin::Pin;
use core::ptr::NonNull;
use core::task::{Context, Poll};

pub use futures_util::{SinkExt as _, StreamExt as _};
use ndarray::Array3;
pub use scoped_stream_sink::{LocalScopedStream, LocalStreamInner, Sink, Stream};

#[derive(Debug, Clone)]
#[repr(C)]
pub struct State {
    data_ptr: *mut u32,
    pub drone: Drone,

    pub data: Array3<u32>,
}

unsafe impl Send for State {}
unsafe impl Sync for State {}

impl State {
    pub fn new(size_x: usize, size_y: usize, size_z: usize) -> Self {
        let mut data = Array3::default((size_x, size_y, size_z));
        Self {
            data_ptr: data.as_mut_ptr(),
            drone: Drone::new(),

            data,
        }
    }

    pub fn update_export(&mut self) {
        self.data_ptr = self.data.as_mut_ptr();
    }
}

pub const INVENTORY_SIZE: usize = 9;

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct Drone {
    pub x: usize,
    pub y: usize,
    pub z: usize,

    pub command: Command,
    pub inventory: [Inventory; INVENTORY_SIZE],
}

impl Drone {
    pub const fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            z: 0,
            command: Command::Noop,
            inventory: [Inventory::new(None, 0); INVENTORY_SIZE],
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(u8)]
pub enum Command {
    #[default]
    Noop,
    Move(Dir),
    BreakBlock(Dir),
    PlaceBlock(Dir, u8),
    SendItem(Dir, u8),
    RecvItem(Dir, u8),
    Restack,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Dir {
    #[default]
    Noop,
    Up,
    Down,
    Left,
    Right,
    Front,
    Back,
}

impl Dir {
    pub fn move_coord(
        &self,
        size: &(usize, usize, usize),
        coord: (usize, usize, usize),
    ) -> Option<(usize, usize, usize)> {
        Some(match self {
            Self::Noop => coord,
            Self::Up if coord.1 + 1 < size.1 => (coord.0, coord.1 + 1, coord.2),
            Self::Down if coord.1 > 0 => (coord.0, coord.1 - 1, coord.2),
            Self::Left if coord.0 + 1 < size.0 => (coord.0 + 1, coord.1, coord.2),
            Self::Right if coord.0 > 0 => (coord.0 - 1, coord.1, coord.2),
            Self::Back if coord.2 + 1 < size.2 => (coord.0, coord.1, coord.2 + 1),
            Self::Front if coord.2 > 0 => (coord.0, coord.1, coord.2 - 1),
            _ => return None,
        })
    }
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct Inventory {
    pub item_id: Option<NonZeroU8>,
    pub count: u8,
}

impl Inventory {
    pub const fn new(item_id: Option<NonZeroU8>, count: u8) -> Self {
        Self {
            count: if item_id.is_none() { 0 } else { count },
            item_id,
        }
    }
}

pub struct Runtime<'env> {
    pub state: State,
    pub stream: Option<LocalScopedStream<'env, Command>>,
}

pub struct RuntimeInner<'scope, 'env> {
    inner: Pin<&'scope mut LocalStreamInner<'scope, 'env, Command>>,
    state: NonNull<State>,
}

unsafe impl<'scope, 'env> Send for RuntimeInner<'scope, 'env> {}
unsafe impl<'scope, 'env> Sync for RuntimeInner<'scope, 'env> {}

impl<'scope, 'env> RuntimeInner<'scope, 'env> {
    pub fn new(
        inner: Pin<&'scope mut LocalStreamInner<'scope, 'env, Command>>,
        state: NonNull<State>,
    ) -> Self {
        Self { inner, state }
    }
}

impl<'scope, 'env> Deref for RuntimeInner<'scope, 'env> {
    type Target = State;

    fn deref(&self) -> &State {
        // SAFETY: State is not accessed by outer
        unsafe { &*self.state.as_ptr() }
    }
}

impl<'scope, 'env> Sink<Command> for RuntimeInner<'scope, 'env> {
    type Error = <LocalStreamInner<'scope, 'env, Command> as Sink<Command>>::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.get_mut().inner.as_mut().poll_ready(cx)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.get_mut().inner.as_mut().poll_flush(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Command) -> Result<(), Self::Error> {
        self.get_mut().inner.as_mut().start_send(item)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.get_mut().inner.as_mut().poll_close(cx)
    }
}

impl<'env> Runtime<'env> {
    pub fn new(size_x: usize, size_y: usize, size_z: usize) -> Self {
        Self {
            state: State::new(size_x, size_y, size_z),
            stream: None,
        }
    }
}

#[macro_export]
macro_rules! drone {
    (($ctx:ident) $b:block) => {
        static mut STATE: Option<$crate::Runtime> = None;

        #[export_name = "init"]
        pub extern "C" fn __init(size_x: usize, size_y: usize, size_z: usize) -> *mut $crate::State {
            unsafe {
                STATE = Some($crate::Runtime::new(size_x, size_y, size_z));
                (&mut STATE.as_mut().unwrap_unchecked().state) as _
            }
        }

        fn __inner<'scope, 'env>(mut $ctx: $crate::RuntimeInner<'scope, 'env>) -> core::pin::Pin<Box<dyn core::future::Future<Output = ()> + 'scope>> {
            Box::pin(async move $b) as _
        }

        #[export_name = "step"]
        pub extern "C" fn __step() {
            use $crate::Stream as _;

            /// Create a null waker. It does nothing when waken.
            fn nil_waker() -> core::task::Waker {
                fn raw() -> core::task::RawWaker {
                    core::task::RawWaker::new(core::ptr::NonNull::dangling().as_ptr(), &VTABLE)
                }

                unsafe fn clone(_: *const ()) -> core::task::RawWaker {
                    raw()
                }
                unsafe fn wake(_: *const ()) {}
                unsafe fn wake_by_ref(_: *const ()) {}
                unsafe fn drop(_: *const ()) {}

                static VTABLE: core::task::RawWakerVTable = core::task::RawWakerVTable::new(clone, wake, wake_by_ref, drop);

                unsafe { core::task::Waker::from_raw(raw()) }
            }

            let state = unsafe { STATE.as_mut().unwrap_unchecked() };
            state.state.drone.command = $crate::Command::Noop;
            let waker = nil_waker();
            let mut cx = core::task::Context::from_waker(&waker);

            for _ in 0..2 {
                let stream = core::pin::Pin::new(state.stream.get_or_insert_with(
                    || $crate::LocalScopedStream::new(
                        |inner| __inner($crate::RuntimeInner::new(inner, (&state.state).into())),
                    ),
                ));
                state.state.drone.command = match stream.poll_next(&mut cx) {
                    core::task::Poll::Pending => $crate::Command::Noop,
                    core::task::Poll::Ready(None) => {
                        state.stream = None;
                        continue;
                    },
                    core::task::Poll::Ready(Some(v)) => v,
                };
                return;
            }
        }
    };
}
