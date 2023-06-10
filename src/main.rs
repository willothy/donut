use std::{
    alloc::GlobalAlloc,
    fmt::Debug,
    sync::atomic::{AtomicUsize, Ordering},
};

extern crate alloc;

pub struct RingBuf<T> {
    pub items: *mut T,
    pub head: AtomicUsize,
    pub tail: AtomicUsize,
    pub size: AtomicUsize,
    pub len: AtomicUsize,
}

impl<T> RingBuf<T> {
    pub fn new(cap: usize) -> Self {
        Self {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            len: AtomicUsize::new(0),
            size: AtomicUsize::new(cap),
            items: unsafe {
                alloc::alloc::alloc(alloc::alloc::Layout::array::<T>(cap).expect("to allocate"))
                    .cast()
            },
        }
    }

    pub fn push(&self, item: T) {
        let size = self.size.load(Ordering::Relaxed);
        let tail = self
            .tail
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some((v + 1) % size)
            })
            .expect("bruh");
        unsafe {
            core::ptr::write(self.items.add(tail), item);
        }
        self.len
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some((v + 1).min(size))
            })
            .expect("bruh");
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len.load(Ordering::Relaxed) == 0 {
            return None;
        }

        let head = match self
            .head
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some((v + 1) % self.size.load(Ordering::Relaxed))
            }) {
            Ok(v) => v,
            Err(v) => v,
        };
        let item = unsafe { core::ptr::read(self.items.add(head)) };
        self.len.fetch_sub(1, Ordering::Relaxed);
        Some(item)
    }

    fn resize(&mut self, new_size: usize) {
        self.items = unsafe {
            alloc::alloc::realloc(
                self.items.cast(),
                alloc::alloc::Layout::array::<T>(self.size.swap(new_size, Ordering::Relaxed))
                    .expect("to reallocate"),
                new_size,
            )
            .cast()
        };
        self.head.store(0, Ordering::Relaxed);
        self.tail
            .store(self.len.load(Ordering::Relaxed), Ordering::Relaxed);
    }
}

impl<T> Drop for RingBuf<T> {
    fn drop(&mut self) {
        for i in self.head.load(Ordering::Relaxed)..self.tail.load(Ordering::Relaxed) {
            unsafe {
                core::ptr::drop_in_place(self.items.add(i));
            }
        }

        unsafe {
            alloc::alloc::dealloc(
                self.items.cast(),
                alloc::alloc::Layout::array::<T>(self.size.load(Ordering::Relaxed))
                    .expect("to deallocate"),
            );
        }
    }
}

pub struct Deque<T> {
    items: *mut T,
    head: usize,
    tail: usize,
    len: usize,
    size: usize,
}

impl<T> Deque<T> {
    pub fn new() -> Self {
        Self {
            items: unsafe {
                alloc::alloc::alloc(core::alloc::Layout::array::<T>(4).expect("to allocate")).cast()
            },
            head: 2,
            tail: 2,
            len: 0,
            size: 4,
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            items: unsafe {
                alloc::alloc::alloc(core::alloc::Layout::array::<T>(cap).expect("to allocate"))
                    .cast()
            },
            head: cap / 2,
            tail: cap / 2,
            len: 0,
            size: cap,
        }
    }

    pub fn new_in(cap: usize, alloc: &impl GlobalAlloc) -> Self {
        Self {
            items: unsafe {
                alloc
                    .alloc(core::alloc::Layout::array::<T>(cap).expect("to allocate"))
                    .cast()
            },
            head: cap / 2,
            tail: cap / 2,
            len: 0,
            size: cap,
        }
    }

    pub fn push_front(&mut self, item: T) {
        if self.head == 0 {
            self.resize(self.size * 2);
        }

        self.head -= 1;
        unsafe {
            core::ptr::write(self.items.add(self.head), item);
        }
        self.len += 1;
    }

    pub fn push_back(&mut self, item: T) {
        if self.tail == self.size {
            self.resize(self.size * 2);
        }

        unsafe {
            core::ptr::write(self.items.add(self.tail), item);
        }
        self.tail += 1;
        self.len += 1;
    }

    pub fn resize(&mut self, new_size: usize) {
        let new_alloc: *mut T = unsafe {
            alloc::alloc::alloc(core::alloc::Layout::array::<T>(new_size).expect("to allocate"))
                .cast()
        };
        unsafe {
            core::ptr::copy_nonoverlapping(
                self.items.add(self.head),
                new_alloc.add(self.head + self.size),
                self.len,
            );
            alloc::alloc::dealloc(
                self.items.cast(),
                alloc::alloc::Layout::array::<T>(self.size).expect("to deallocate"),
            );
        }
        self.items = new_alloc;
        self.size = new_size;
        self.head = self.head + self.size / 2;
        self.tail = self.tail + self.size / 2;
    }
}

impl<T> Drop for Deque<T> {
    fn drop(&mut self) {
        unsafe {
            for i in self.head..self.tail {
                core::ptr::drop_in_place(self.items.add(i));
            }
            alloc::alloc::dealloc(
                self.items.cast(),
                alloc::alloc::Layout::array::<T>(self.size).expect("to deallocate"),
            );
        }
    }
}

impl<T: Debug> Debug for Deque<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = Vec::new();
        if self.head > self.tail {
            for i in self.head..self.size {
                unsafe { s.push(self.items.add(i).as_ref().unwrap()) };
            }
            for i in 0..self.tail {
                unsafe { s.push(self.items.add(i).as_ref().unwrap()) };
            }
        } else {
            for i in self.head..self.tail {
                unsafe { s.push(self.items.add(i).as_ref().unwrap()) };
            }
        }
        f.debug_struct("Deque")
            .field("items", &s)
            .field("head", &self.head)
            .field("tail", &self.tail)
            .field("len", &self.len)
            .field("capacity", &self.size)
            .finish()
    }
}

fn main() {
    let mut d: Deque<i32> = Deque::new();
    d.push_front(5);
    d.push_front(6);
    d.push_front(7);
    d.push_back(11);
    d.push_back(12);
    d.push_front(9);
    d.push_front(9);
    d.push_front(9);
    d.push_front(9);

    println!("{:?}", d);
}
