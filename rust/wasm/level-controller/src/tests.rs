// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::drone::*;
use super::*;

use anyhow::Error;
use itertools::Itertools as _;

const SEED: u64 = 0x7EA12_C12AF7ED;

fn update_all_drones(state: &mut State) {
    state.data &= !OCCUPIED_FLAG;
    for d in &state.drones {
        state.data[(d.x, d.y, d.z)] |= OCCUPIED_FLAG;
    }
}

fn print_all_drone_coords(state: &State) {
    for i in &state.drones {
        println!("{} {} {}", i.x, i.y, i.z);
    }
}

#[test]
fn test_move_one() -> Result<(), Error> {
    let mut state = State::new(SEED, [2, 1, 2], 16, 1, 1);

    state.drones[0] = Drone {
        x: 0,
        y: 0,
        z: 0,
        command: Command::Move(Dir::Left),
        ..Drone::default()
    };
    update_all_drones(&mut state);

    execute_commands(&mut state);

    print_all_drone_coords(&state);
    assert_eq!(state.drones[0].x, 1);
    assert_eq!(state.drones[0].y, 0);
    assert_eq!(state.drones[0].z, 0);

    Ok(())
}

#[test]
fn test_move_swap() -> Result<(), Error> {
    let mut state = State::new(SEED, [2, 1, 2], 16, 2, 1);

    fn f(state: &mut State, a: usize, b: usize) {
        state.drones[a] = Drone {
            x: 0,
            y: 0,
            z: 0,
            command: Command::Move(Dir::Left),
            ..Drone::default()
        };
        state.drones[b] = Drone {
            x: 1,
            y: 0,
            z: 0,
            command: Command::Move(Dir::Right),
            ..Drone::default()
        };
        update_all_drones(state);

        execute_commands(state);

        print_all_drone_coords(&state);
        assert_eq!(state.drones[a].x, 1);
        assert_eq!(state.drones[a].y, 0);
        assert_eq!(state.drones[a].z, 0);
        assert_eq!(state.drones[b].x, 0);
        assert_eq!(state.drones[b].y, 0);
        assert_eq!(state.drones[b].z, 0);
    }

    f(&mut state, 0, 1);
    f(&mut state, 1, 0);

    Ok(())
}

#[test]
fn test_move_loop() -> Result<(), Error> {
    let mut state = State::new(SEED, [2, 2, 2], 16, 4, 1);

    fn f(state: &mut State, a: usize, b: usize, c: usize, d: usize) {
        state.drones[a] = Drone {
            x: 0,
            y: 0,
            z: 0,
            command: Command::Move(Dir::Left),
            ..Drone::default()
        };
        state.drones[b] = Drone {
            x: 1,
            y: 0,
            z: 0,
            command: Command::Move(Dir::Up),
            ..Drone::default()
        };
        state.drones[c] = Drone {
            x: 1,
            y: 1,
            z: 0,
            command: Command::Move(Dir::Right),
            ..Drone::default()
        };
        state.drones[d] = Drone {
            x: 0,
            y: 1,
            z: 0,
            command: Command::Move(Dir::Down),
            ..Drone::default()
        };
        update_all_drones(state);

        execute_commands(state);

        print_all_drone_coords(&state);
        assert_eq!(state.drones[a].x, 1);
        assert_eq!(state.drones[a].y, 0);
        assert_eq!(state.drones[a].z, 0);
        assert_eq!(state.drones[b].x, 1);
        assert_eq!(state.drones[b].y, 1);
        assert_eq!(state.drones[b].z, 0);
        assert_eq!(state.drones[c].x, 0);
        assert_eq!(state.drones[c].y, 1);
        assert_eq!(state.drones[c].z, 0);
        assert_eq!(state.drones[d].x, 0);
        assert_eq!(state.drones[d].y, 0);
        assert_eq!(state.drones[d].z, 0);
    }

    for v in (0..4).permutations(4) {
        let [a, b, c, d] = <[_; 4]>::try_from(&*v).unwrap();
        println!("a: {a} b: {b} c: {c} d: {d}");
        f(&mut state, a, b, c, d);
    }

    Ok(())
}

#[test]
fn test_move_tree() -> Result<(), Error> {
    let mut state = State::new(SEED, [3, 2, 3], 16, 3, 1);

    fn f(state: &mut State, a: usize, b: usize, c: usize) {
        state.drones[a] = Drone {
            x: 0,
            y: 0,
            z: 0,
            command: Command::Move(Dir::Left),
            ..Drone::default()
        };
        state.drones[b] = Drone {
            x: 1,
            y: 0,
            z: 0,
            command: Command::Move(Dir::Left),
            ..Drone::default()
        };
        state.drones[c] = Drone {
            x: 1,
            y: 1,
            z: 0,
            command: Command::Move(Dir::Down),
            ..Drone::default()
        };
        update_all_drones(state);

        execute_commands(state);

        print_all_drone_coords(&state);
        assert_eq!(state.drones[a].x, 1);
        assert_eq!(state.drones[a].y, 0);
        assert_eq!(state.drones[a].z, 0);
        assert_eq!(state.drones[b].x, 2);
        assert_eq!(state.drones[b].y, 0);
        assert_eq!(state.drones[b].z, 0);
        assert_eq!(state.drones[c].x, 1);
        assert_eq!(state.drones[c].y, 1);
        assert_eq!(state.drones[c].z, 0);
    }

    for v in (0..3).permutations(3) {
        let [a, b, c] = <[_; 3]>::try_from(&*v).unwrap();
        println!("a: {a} b: {b} c: {c}");
        f(&mut state, a, b, c);
    }

    Ok(())
}

#[test]
fn test_move_fail_oob() -> Result<(), Error> {
    let mut state = State::new(SEED, [2, 2, 2], 16, 1, 1);

    fn f(state: &mut State, [x, y, z]: [usize; 3], dir: Dir) {
        state.drones[0] = Drone {
            x,
            y,
            z,
            command: Command::Move(dir),
            ..Drone::default()
        };
        update_all_drones(state);

        execute_commands(state);

        print_all_drone_coords(&state);
        assert_eq!(state.drones[0].x, x);
        assert_eq!(state.drones[0].y, y);
        assert_eq!(state.drones[0].z, z);
    }
    for y in 0..2 {
        for z in 0..2 {
            f(&mut state, [0, y, z], Dir::Right);
            f(&mut state, [1, y, z], Dir::Left);
        }
    }
    for x in 0..2 {
        for z in 0..2 {
            f(&mut state, [x, 0, z], Dir::Down);
            f(&mut state, [x, 1, z], Dir::Up);
        }
    }
    for x in 0..2 {
        for y in 0..2 {
            f(&mut state, [x, y, 0], Dir::Front);
            f(&mut state, [x, y, 1], Dir::Back);
        }
    }

    Ok(())
}

#[test]
fn test_move_fail() -> Result<(), Error> {
    let mut state = State::new(SEED, [2, 1, 2], 16, 2, 1);

    state.drones[0] = Drone {
        x: 0,
        y: 0,
        z: 0,
        command: Command::Move(Dir::Left),
        ..Drone::default()
    };
    state.drones[1] = Drone {
        x: 1,
        y: 0,
        z: 0,
        ..Drone::default()
    };
    update_all_drones(&mut state);

    execute_commands(&mut state);

    print_all_drone_coords(&state);
    assert_eq!(state.drones[0].x, 0);
    assert_eq!(state.drones[0].y, 0);
    assert_eq!(state.drones[0].z, 0);
    assert_eq!(state.drones[1].x, 1);
    assert_eq!(state.drones[1].y, 0);
    assert_eq!(state.drones[1].z, 0);

    Ok(())
}

#[test]
fn test_move_fail_chain() -> Result<(), Error> {
    let mut state = State::new(SEED, [2, 2, 2], 16, 4, 1);

    state.drones[0] = Drone {
        x: 0,
        y: 0,
        z: 0,
        command: Command::Move(Dir::Left),
        ..Drone::default()
    };
    state.drones[1] = Drone {
        x: 1,
        y: 0,
        z: 0,
        command: Command::Move(Dir::Up),
        ..Drone::default()
    };
    state.drones[2] = Drone {
        x: 1,
        y: 1,
        z: 0,
        command: Command::Move(Dir::Right),
        ..Drone::default()
    };
    state.drones[3] = Drone {
        x: 0,
        y: 1,
        z: 0,
        command: Command::Move(Dir::Right),
        ..Drone::default()
    };
    update_all_drones(&mut state);

    execute_commands(&mut state);

    print_all_drone_coords(&state);
    assert_eq!(state.drones[0].x, 0);
    assert_eq!(state.drones[0].y, 0);
    assert_eq!(state.drones[0].z, 0);
    assert_eq!(state.drones[1].x, 1);
    assert_eq!(state.drones[1].y, 0);
    assert_eq!(state.drones[1].z, 0);
    assert_eq!(state.drones[2].x, 1);
    assert_eq!(state.drones[2].y, 1);
    assert_eq!(state.drones[2].z, 0);
    assert_eq!(state.drones[3].x, 0);
    assert_eq!(state.drones[3].y, 1);
    assert_eq!(state.drones[3].z, 0);

    Ok(())
}

#[test]
fn test_move_fail_tree() -> Result<(), Error> {
    let mut state = State::new(SEED, [2, 2, 2], 16, 4, 1);

    state.drones[0] = Drone {
        x: 0,
        y: 0,
        z: 0,
        command: Command::Move(Dir::Left),
        ..Drone::default()
    };
    state.drones[1] = Drone {
        x: 1,
        y: 0,
        z: 0,
        command: Command::Noop,
        ..Drone::default()
    };
    state.drones[2] = Drone {
        x: 1,
        y: 1,
        z: 0,
        command: Command::Move(Dir::Down),
        ..Drone::default()
    };
    state.drones[3] = Drone {
        x: 0,
        y: 1,
        z: 0,
        command: Command::Move(Dir::Left),
        ..Drone::default()
    };
    update_all_drones(&mut state);

    execute_commands(&mut state);

    print_all_drone_coords(&state);
    assert_eq!(state.drones[0].x, 0);
    assert_eq!(state.drones[0].y, 0);
    assert_eq!(state.drones[0].z, 0);
    assert_eq!(state.drones[1].x, 1);
    assert_eq!(state.drones[1].y, 0);
    assert_eq!(state.drones[1].z, 0);
    assert_eq!(state.drones[2].x, 1);
    assert_eq!(state.drones[2].y, 1);
    assert_eq!(state.drones[2].z, 0);
    assert_eq!(state.drones[3].x, 0);
    assert_eq!(state.drones[3].y, 1);
    assert_eq!(state.drones[3].z, 0);

    Ok(())
}
