// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use rand::Rng;

use super::drone::Inventory;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockType {
    Empty,
    Full,
    Blade,
}

macro_rules! blocks {
    (#dist $r:ident ..) => {$r.gen()};
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
                Inventory::new($itd.into(), blocks!(#dist $r $d))
            ),*]);
        }
    };
    (place $ty:ident $id:literal $it:literal) => {
        if $ty == $it {
            return Some($id);
        }
    };
    ($($id:literal : ($t:ident, $uv:tt, $d:tt, $p:tt)),* $(,)?) => {
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

        pub const fn block_place(_it: u8) -> Option<u8> {
            $(blocks!{place (_ty _r _f) $id $p})*
            None
        }
    };
}

blocks! {
    0 : (Empty, _, _, _),
}
