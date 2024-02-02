// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use drone_core::*;

drone! {
    (ctx) {
        // Move right until can't
        loop {
            let prev_x = ctx.drone.x;
            ctx.send(Command::Move(Dir::Right)).await.unwrap();
            if (ctx.drone.x == prev_x) || (ctx.drone.x == 0) {
                break;
            }
        }

        // Move left until can't
        loop {
            let prev_x = ctx.drone.x;
            ctx.send(Command::Move(Dir::Left)).await.unwrap();
            if (ctx.drone.x == prev_x) || (ctx.drone.x == ctx.data.raw_dim()[0] - 1) {
                break;
            }
        }
    }
}
