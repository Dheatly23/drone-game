// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::num::NonZeroU16;

use ndarray::Array3;
use rand::Rng;

use super::drone::Inventory;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockType {
    Empty,
    Full,
    Blade,
}

macro_rules! blocks {
    (#dist $r:ident ..) => {$r.gen(0..Inventory::MAX_STACK)};
    (#dist $r:ident $n:literal) => {$n};
    (#dist $r:ident ..$b:literal) => {$r.gen(0..$b)};
    (#dist $r:ident $a:literal..$b:literal) => {$r.gen($a..$b)};
    ($t:ident $ty:tt $id:literal _) => {};
    (uv $ty:ident $id:literal [$x:literal, $y:literal]) => {
        if $ty == $id {
            return [$x, $y];
        }
    };
    (drop ($ty:ident $r:ident $f:ident) $id:literal [$($itd:literal => $d:tt),* $(,)?]) => {
        if $ty == $id {
            return $f(&mut [$(
                Inventory::new(NonZeroU16::new($itd), blocks!(#dist $r $d))
            ),*]);
        }
    };
    (place ($ty:ident $c:ident $data:ident) $id:literal $it:literal) => {
        if $ty == $it {
            return Some($id);
        }
    };
    (place ($ty:ident $c:ident $data:ident) $id:literal ($it:literal => |$c_:ident, $data_:ident| $b:block)) => {
        let f = |$c_: (usize, usize, usize), $data_: &Array3<u32>| -> bool $b;
        if ($ty == $it) && f($c, $data) {
            return Some($id);
        }
    };
    (tick ($ty:ident $r:ident $c:ident $data:ident) $id:literal |$r_:ident, $c_:ident, $data_:ident| $b:block) => {
        let f = |$r_: &mut R, $c_: (usize, usize, usize), $data_: &Array3<u32>| -> Option<u32> $b;
        if $ty == $id {
            if let Some(b) = f(&mut *$r, $c, &*$data) {
                $data[$c] = b;
            }
        }
    };
    ($($id:literal : ($t:ident, $uv:tt, $d:tt, $p:tt, $rt:tt)),* $(,)?) => {
        pub const fn is_valid(ty: u8) -> bool {
            match ty {
                $($id)|* => true,
                _ => false,
            }
        }

        pub const fn block_type(ty: u8) -> BlockType {
            match ty {
                $($id => BlockType::$t,)*
                _ => BlockType::Empty,
            }
        }

        pub const fn block_uv(_ty: u8) -> [usize; 2] {
            $(blocks!{uv _ty $id $uv})*
            [0, 0]
        }

        pub fn block_drops<R, F, T>(_ty: u8, _r: &mut R, _f: F) -> T
        where
            R: Rng,
            F: FnOnce(&mut [Inventory]) -> T,
            T: Default,
        {
            $(blocks!{drop (_ty _r _f) $id $d})*
            T::default()
        }

        pub const fn block_place(_it: u16, _c: (usize, usize, usize), _data: &Array3<u32>) -> Option<u8> {
            $(blocks!{place (_it _c _data) $id $p})*
            None
        }

        pub fn random_tick<R, F>(_r: &mut R, mut c: F, _data: &mut Array3<u32>)
        where
            R: Rng,
            F: FnMut(&mut R) -> Option<(usize, usize, usize)>,
        {
            while let Some(c) = c(&mut *_r) {
                let Some(&_b) = _data.get(c) else {
                    continue;
                };

                $(blocks!{place (_b _r c _data) $id $rt})*
            }
        }
    };
}

blocks! {
    // Air
    0 : (Empty, _, _, _, _),
    // Dirt
    1 : (Full, [0, 0], [1 => 1], 1, _),
    // Grass
    2 : (Full, [1, 0], [1 => 1], _, _),
}
