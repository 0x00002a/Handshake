use std::{cell::UnsafeCell, fmt::Debug, ptr::NonNull, sync::{Arc, Mutex, OnceLock, RwLock}};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Canceled;

pub struct Handshake<T> {
    // NotNull is & unless deduced otherwise
    common: Arc<RwLock<Option<T>>>,
}

impl<T> Handshake<T> {
    pub fn new() -> (Handshake<T>, Handshake<T>) {
        // check expected to be elided during compilation
        let common = Default::default();
        let h1 = Handshake { common };
        let common = h1.common.clone();
        (h1, Handshake {common})
    }

    pub fn join<U, F: FnOnce(T, T) -> U>(self, value: T, f: F) -> Result<Option<U>, Canceled> {
        let Ok(other) = self.try_pull()? else { return Err(Canceled); };
        Ok(Some(f(other, value)))
    }

    pub fn try_push(self, value: T) -> Result<Result<(), (Self, T)>, T> {
        // access safe lock
        let mut common = self.common.write().unwrap();
        assert!(common.is_none(), "try to push to already pushed handshake");
        common.replace(value);
        drop(common);
        std::mem::forget(self); // consumes `self`
        Ok(Ok(()))
    }

    pub fn try_pull(self) -> Result<Result<T, Self>, Canceled> {
        // access safe lock
        let mut common = self.common.write().unwrap();
        assert!(common.is_none(), "try to push to already pushed handshake");
        if let Some(res) = common.take() {
            Ok(Ok(res))
        }
        else {
            drop(common);
            Ok(Err(self))
        }
    }

    pub fn is_set(&self) -> bool {
        // access safe lock
        self.common.read().unwrap().is_some()
    }
}

impl<T: PartialEq> PartialEq for Handshake<T> {
    fn eq(&self, other: &Self) -> bool {
        *self.common.read().unwrap() == *other.common.read().unwrap()
    }
}


impl<T: Debug> Debug for Handshake<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // access safe lock
        f.debug_struct("Handshake").field("common", unsafe { self.common.as_ref() }).finish()
    }
}

#[cfg(test)]
mod test {
    use std::convert::identity;

    use crate::{Canceled, Handshake};

    #[test]
    fn drop_test() {
        let (u, v) = Handshake::<()>::new();
        drop(u);
        drop(v);

        let (u, v) = Handshake::<()>::new();
        drop(v);
        drop(u)
    }

    #[test]
    fn push_drop_test() {
        #[derive(Debug)]
        struct Loud<'a> {
            flag: &'a mut bool
        }

        impl<'a> Drop for Loud<'a> {
            fn drop(&mut self) {
                *self.flag = true;
            }
        }

        let mut dropped = false;
        let (u, v) = Handshake::<Loud>::new();
        u.try_push(Loud { flag: &mut dropped }).unwrap().unwrap();
        drop(v);

        assert_eq!(dropped, true);
    }

    #[test]
    fn pull_test() {
        let (u, v) = Handshake::<()>::new();
        assert_eq!(u.try_pull(), Ok(Err(v)));

        let (u, v) = Handshake::<()>::new();
        assert_eq!(v.try_pull(), Ok(Err(u)))
    }

    #[test]
    fn push_test() {
        let (u, v) = Handshake::<()>::new();
        assert_eq!(u.try_push(()), Ok(Ok(())));
        drop(v);

        let (u, v) = Handshake::<()>::new();
        assert_eq!(v.try_push(()), Ok(Ok(())));
        drop(u)
    }

    #[test]
    fn double_push_test() {
        let (u, v) = Handshake::<()>::new();
        u.try_push(()).unwrap().unwrap();
        drop(v.try_push(()).unwrap().err().unwrap());

        let (u, v) = Handshake::<()>::new();
        v.try_push(()).unwrap().unwrap();
        drop(u.try_push(()).unwrap().err().unwrap())
    }

    #[test]
    fn pull_cancel_test() {
        let (u, v) = Handshake::<()>::new();
        drop(u);
        assert_eq!(v.try_pull(), Err(Canceled));

        let (u, v) = Handshake::<()>::new();
        drop(v);
        assert_eq!(u.try_pull(), Err(Canceled));
    }

    #[test]
    fn push_cancel_test() {
        let (u, v) = Handshake::<()>::new();
        drop(u);
        assert_eq!(v.try_push(()), Err(()));

        let (u, v) = Handshake::<()>::new();
        drop(v);
        assert_eq!(u.try_push(()), Err(()));
    }

    #[test]
    fn push_pull_test() {
        let (u, v) = Handshake::<()>::new();
        u.try_push(()).unwrap().unwrap();
        v.try_pull().unwrap().unwrap();

        let (u, v) = Handshake::<()>::new();
        v.try_push(()).unwrap().unwrap();
        u.try_pull().unwrap().unwrap()
    }

    #[test]
    fn join_test() {
        let (u, v) = Handshake::<()>::new();
        assert_eq!(u.join((), |_, _| ()).unwrap(), None);
        assert_eq!(v.join((), |_, _| ()).unwrap(), Some(()));

        let (u, v) = Handshake::<()>::new();
        assert_eq!(v.join((), |_, _| ()).unwrap(), None);
        assert_eq!(u.join((), |_, _| ()).unwrap(), Some(()))
    }

    #[test]
    // Due to the innefective `OnceLock` API and
    // the requirement to keep `self` around for either `std::mem::forget(self)` or return
    // there is a break in aliasing rules as a `&` is coexisting with a `&mut` (even though the `&` is not used).
    // This means that these functions (join, try_push, try_pull) do not pass tests involving miri,
    // however it would appear they are still perfectly safe.
    fn collision_check() {
        use rand::prelude::*;
        const N: usize = 64;

        let mut left: Vec<Handshake<usize>> = vec![];
        let mut right: Vec<Handshake<usize>> = vec![];
        for _ in 0..N {
            let (u, v) = Handshake::<usize>::new();
            left.push(u);
            right.push(v)
        }
        let mut rng = rand::thread_rng();
        left.shuffle(&mut rng);
        right.shuffle(&mut rng);
        let left_thread = std::thread::spawn(|| left
            .into_iter()
            .enumerate()
            .map(|(n, u)| {u.join(n, |x, y| (x, y)).unwrap()})
            .filter_map(identity).collect::<Vec<(usize, usize)>>()
        );
        let right_thread = std::thread::spawn(|| right
            .into_iter()
            .enumerate()
            .map(|(n, v)| {v.join(n, |x, y| (x, y)).unwrap()})
            .filter_map(identity).collect::<Vec<(usize, usize)>>()
        );
        let total = left_thread.join().unwrap().len() + right_thread.join().unwrap().len();
        assert_eq!(total, N)
    }
}