// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::num::NonZeroU16;

use drone_core::ndarray::Axis;
use drone_core::*;

drone! {
    (ctx) {
        print_log(format_args!("Starting"));

        // Find air
        let Some(((x, z), _)) = ctx.data.index_axis(Axis(1), 0).indexed_iter().find(|&(_, &b)| b == 0) else {
            print_log(format_args!("All block placed!"));
            return;
        };
        print_log(format_args!("Found at {x}, {z}"));

        // Move to place
        while (ctx.drone.x != x) || (ctx.drone.z != z) {
            let d = if ctx.drone.x < x {
                Dir::Left
            } else if ctx.drone.x > x {
                Dir::Right
            } else if ctx.drone.z < z {
                Dir::Back
            } else {
                Dir::Front
            };
            print_log(format_args!("Moving {d}"));
            ctx.send(Command::Move(d)).await.unwrap();

            if ctx.data[(x, 0, z)] != 0 {
                return;
            }
        }

        let Some((i, _)) = ctx.drone.inventory.iter().enumerate().find(|&(_, v)| v.item_id == NonZeroU16::new(1)) else {
            print_log(format_args!("Has no item in inventory"));
            return;
        };
        print_log(format_args!("Placing block at slot {i}"));
        ctx.send(Command::PlaceBlock(Dir::Down, i as _)).await.unwrap();
    }
}
