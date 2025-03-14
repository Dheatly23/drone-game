use std::mem::{MaybeUninit, take, transmute};
use std::ptr::dangling;

use level_state::{BlockEntityData, LevelState};
use util_wasm::buffer;

use crate::util::WriteBuf;

#[link(wasm_import_module = "host")]
unsafe extern "C" {
    #[link_name = "entity_removed"]
    fn _entity_removed(a0: u32, a1: u32, a2: u32, a3: u32);
    #[link_name = "entity_iron_ore"]
    fn _entity_iron_ore(a0: u32, a1: u32, a2: u32, a3: u32, x: u32, y: u32, z: u32, qty: u64);
    #[link_name = "entity_drone"]
    fn _entity_drone(a0: u32, a1: u32, a2: u32, a3: u32, x: u32, y: u32, z: u32);
    #[link_name = "entity_central_tower"]
    fn _entity_central_tower(
        a0: u32,
        a1: u32,
        a2: u32,
        a3: u32,
        x: u32,
        y: u32,
        z: u32,
        p: *const ExportCentralTower,
    );
}

#[repr(C)]
struct ExportCentralTower {
    exec_p: *const u8,
    exec_n: usize,
    args_p: *const Arg,
    args_n: usize,
    env_p: *const Env,
    env_n: usize,
}

#[repr(C)]
struct Arg {
    p: *const u8,
    n: usize,
}

#[repr(C)]
struct Env {
    kp: *const u8,
    kn: usize,
    vp: *const u8,
    vn: usize,
}

fn from_uuid(v: u128) -> [u32; 4] {
    [
        (v & 0xffff_ffff) as u32,
        ((v >> 32) & 0xffff_ffff) as u32,
        ((v >> 64) & 0xffff_ffff) as u32,
        ((v >> 96) & 0xffff_ffff) as u32,
    ]
}

unsafe fn write_central_tower(
    exec: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
) -> ExportCentralTower {
    let buf = unsafe { buffer() };
    let (args_env, strings) = buf.split_at_mut(1024 * 1024);

    struct Strings<'a>(&'a mut [MaybeUninit<u8>]);

    impl Strings<'_> {
        fn push_str(&mut self, s: String) -> (*const u8, usize) {
            let b;
            (b, self.0) = take(&mut self.0).split_at_mut(s.len());
            b.copy_from_slice(unsafe {
                transmute::<&[u8], &[std::mem::MaybeUninit<u8>]>(s.as_bytes())
            });
            (b as *const [MaybeUninit<u8>] as *const u8, b.len())
        }
    }

    let mut strings = Strings(strings);

    let mut data = ExportCentralTower {
        exec_p: dangling(),
        exec_n: 0,
        args_p: dangling(),
        args_n: 0,
        env_p: dangling(),
        env_n: 0,
    };

    (data.exec_p, data.exec_n) = strings.push_str(exec);
    let mut args_out = WriteBuf::new(args_env);
    args_out.extend(args.into_iter().map(|s| {
        let (p, n) = strings.push_str(s);
        Arg { p, n }
    }));
    data.args_p = args_out.as_ptr();
    data.args_n = args_out.len();
    let mut env_out = WriteBuf::new(args_out.rest());
    env_out.extend(env.into_iter().map(|(k, v)| {
        let (kp, kn) = strings.push_str(k);
        let (vp, vn) = strings.push_str(v);
        Env { kp, kn, vp, vn }
    }));
    data.env_p = env_out.as_ptr();
    data.env_n = env_out.len();

    data
}

pub fn update_entity(level: &mut LevelState) {
    for id in level.block_entities_mut().pop_removed() {
        let [a0, a1, a2, a3] = from_uuid(id.as_u128());
        unsafe { _entity_removed(a0, a1, a2, a3) }
    }

    for (id, e) in level.block_entities_mut().entries_mut() {
        if !e.is_dirty() {
            continue;
        }
        e.mark_clean();

        let [a0, a1, a2, a3] = from_uuid(id.as_u128());

        match &mut e.data {
            BlockEntityData::IronOre(v) => unsafe {
                _entity_iron_ore(a0, a1, a2, a3, e.x as _, e.y as _, e.z as _, v.quantity)
            },
            BlockEntityData::Drone(_) => unsafe {
                _entity_drone(a0, a1, a2, a3, e.x as _, e.y as _, e.z as _)
            },
            BlockEntityData::CentralTower(v) => {
                let exec = take(&mut v.exec);
                let args = take(&mut v.args);
                let env = take(&mut v.env);

                unsafe {
                    let v = write_central_tower(exec, args, env);
                    _entity_central_tower(a0, a1, a2, a3, e.x as _, e.y as _, e.z as _, &v)
                }
            }
            _ => continue,
        }
    }
}
