// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::BTreeMap;
use std::ops::{Index, IndexMut};
use std::rc::Rc;

use ringbuffer::{ConstGenericRingBuffer, RingBuffer};

const QUEUE_SIZE: usize = 64;

#[derive(Debug, Default)]
pub struct Subscriber {
    in_queue: ConstGenericRingBuffer<(Rc<[u8]>, Rc<[u8]>), QUEUE_SIZE>,
    out_queue: ConstGenericRingBuffer<(Rc<[u8]>, Rc<[u8]>), QUEUE_SIZE>,
}

impl Subscriber {
    fn publish(&mut self, key: Rc<[u8]>, msg: Rc<[u8]>) {
        self.in_queue.enqueue((key, msg));
    }

    pub fn pop(&mut self) -> Option<(Rc<[u8]>, Rc<[u8]>)> {
        self.out_queue.dequeue()
    }

    fn transfer(&mut self) {
        self.out_queue.extend(self.in_queue.drain());
    }
}

#[derive(Debug, Default)]
pub struct PubSub {
    subscribers: Vec<Box<Subscriber>>,
    listeners: BTreeMap<Rc<[u8]>, Vec<usize>>,
}

impl Index<usize> for PubSub {
    type Output = Subscriber;

    fn index(&self, i: usize) -> &Subscriber {
        &self.subscribers[i]
    }
}

impl IndexMut<usize> for PubSub {
    fn index_mut(&mut self, i: usize) -> &mut Subscriber {
        &mut self.subscribers[i]
    }
}

impl PubSub {
    pub const fn new() -> Self {
        Self {
            subscribers: Vec::new(),
            listeners: BTreeMap::new(),
        }
    }

    pub fn add_subscribers(&mut self, n: usize) {
        self.subscribers
            .resize_with(self.subscribers.len() + n, Default::default);
    }

    pub fn subscriber_listen<K>(&mut self, i: usize, key: K)
    where
        K: AsRef<[u8]> + Into<Rc<[u8]>>,
    {
        assert!(i < self.subscribers.len());

        if let Some(v) = self.listeners.get_mut(key.as_ref()) {
            if let Err(x) = v.binary_search(&i) {
                v.insert(x, i);
            }
        } else {
            self.listeners.insert(key.into(), vec![i]);
        }
    }

    pub fn publish<K, M>(&mut self, key: K, msg: M)
    where
        K: AsRef<[u8]>,
        M: Into<Rc<[u8]>>,
    {
        let Some((key, v)) = self.listeners.get_key_value(key.as_ref()) else {
            return;
        };

        let msg = msg.into();
        for &i in v {
            self.subscribers[i].publish(key.clone(), msg.clone());
        }
    }

    pub fn transfer(&mut self) {
        for s in &mut self.subscribers {
            s.transfer();
        }
    }
}
