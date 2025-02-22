use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Wake, Waker};

pub struct Executor<O> {
    futures: RefCell<Vec<Entry<O>>>,
}

struct Entry<O> {
    fut: Pin<Box<dyn Future<Output = O>>>,
    flag: Arc<SimpleWaker>,
}

struct SimpleWaker {
    flag: AtomicBool,
}

impl Wake for SimpleWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.flag.store(true, Ordering::Release);
    }
}

impl<O> Default for Executor<O> {
    fn default() -> Self {
        Self::new()
    }
}

impl<O> Executor<O> {
    pub const fn new() -> Self {
        Self {
            futures: RefCell::new(Vec::new()),
        }
    }

    pub fn register(&self, mut fut: Pin<Box<dyn Future<Output = O>>>) -> Option<O> {
        let flag = Arc::new(SimpleWaker {
            flag: AtomicBool::new(false),
        });
        if let Poll::Ready(o) = fut
            .as_mut()
            .poll(&mut Context::from_waker(&Waker::from(flag.clone())))
        {
            return Some(o);
        }

        self.futures.borrow_mut().push(Entry { fut, flag });
        None
    }

    pub fn run(&self) -> impl '_ + Iterator<Item = O> {
        struct It<'a, O> {
            this: &'a Executor<O>,
            ix: usize,
        }

        impl<O> Iterator for It<'_, O> {
            type Item = O;

            fn next(&mut self) -> Option<Self::Item> {
                loop {
                    let mut guard = self.this.futures.borrow_mut();
                    self.ix = self.ix.min(guard.len()).checked_sub(1)?;
                    let mut e = guard.swap_remove(self.ix);
                    drop(guard);

                    if e.flag.flag.swap(false, Ordering::SeqCst) {
                        if let Poll::Ready(o) = e
                            .fut
                            .as_mut()
                            .poll(&mut Context::from_waker(&Waker::from(e.flag.clone())))
                        {
                            self.ix += 1;
                            return Some(o);
                        }
                    }

                    self.this.futures.borrow_mut().push(e);
                }
            }
        }

        It {
            ix: self.futures.borrow().len(),
            this: self,
        }
    }
}
