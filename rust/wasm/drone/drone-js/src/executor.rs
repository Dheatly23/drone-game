use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::mem::swap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context as FutContext, Poll, Wake, Waker};

use boa_engine::job::{FutureJob, JobQueue, NativeJob};
use boa_engine::prelude::*;

struct Executor<O> {
    futures: RefCell<Vec<Option<Entry<O>>>>,
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
    const fn new() -> Self {
        Self {
            futures: RefCell::new(Vec::new()),
        }
    }

    fn register(&self, mut fut: Pin<Box<dyn Future<Output = O>>>) -> Option<O> {
        let flag = Arc::new(SimpleWaker {
            flag: AtomicBool::new(false),
        });
        if let Poll::Ready(o) = fut
            .as_mut()
            .poll(&mut FutContext::from_waker(&Waker::from(flag.clone())))
        {
            return Some(o);
        }

        self.futures.borrow_mut().push(Some(Entry { fut, flag }));
        None
    }

    fn run(&self) -> impl '_ + Iterator<Item = O> {
        struct It<'a, O> {
            this: Option<&'a Executor<O>>,
            ix: usize,
        }

        impl<O> Iterator for It<'_, O> {
            type Item = O;

            fn next(&mut self) -> Option<Self::Item> {
                let this = self.this?;

                loop {
                    let mut guard = this.futures.borrow_mut();
                    let (i, mut e) = loop {
                        if self.ix >= guard.len() {
                            guard.retain(Option::is_some);
                            self.this = None;
                            return None;
                        }
                        let i = self.ix;
                        self.ix += 1;
                        if let Some(e) = guard[i].take() {
                            break (i, e);
                        }
                    };
                    drop(guard);

                    if e.flag.flag.swap(false, Ordering::SeqCst) {
                        if let Poll::Ready(o) = e
                            .fut
                            .as_mut()
                            .poll(&mut FutContext::from_waker(&Waker::from(e.flag.clone())))
                        {
                            return Some(o);
                        }
                    }

                    guard = this.futures.borrow_mut();
                    if let v @ None = &mut guard[i] {
                        *v = Some(e);
                    } else {
                        guard.push(Some(e));
                    }
                }
            }
        }

        It {
            ix: 0,
            this: Some(self),
        }
    }
}

pub struct JobRunner {
    jobs: RefCell<(VecDeque<NativeJob>, VecDeque<NativeJob>)>,
    executor: Executor<NativeJob>,
    n_loop: usize,
}

impl JobRunner {
    pub fn new(n_loop: usize) -> Self {
        Self {
            jobs: RefCell::new((VecDeque::new(), VecDeque::new())),
            executor: Default::default(),
            n_loop,
        }
    }
}

impl JobQueue for JobRunner {
    fn enqueue_promise_job(&self, job: NativeJob, _: &mut Context) {
        self.jobs.borrow_mut().1.push_back(job);
    }

    fn enqueue_future_job(&self, future: FutureJob, ctx: &mut Context) {
        if let Some(job) = self.executor.register(future) {
            self.enqueue_promise_job(job, ctx);
        }
    }

    fn run_jobs(&self, ctx: &mut Context) {
        for _ in 0..self.n_loop {
            for job in self.executor.run() {
                self.enqueue_promise_job(job, ctx);
            }

            {
                let mut guard = self.jobs.borrow_mut();
                let (a, b) = &mut *guard;
                swap(a, b);
            }

            loop {
                let Some(job) = self.jobs.borrow_mut().0.pop_front() else {
                    break;
                };

                if let Err(e) = job.call(ctx) {
                    eprintln!("Error in promise: {e}");
                }
            }
        }
    }
}
