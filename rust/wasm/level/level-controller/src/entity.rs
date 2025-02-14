use level_state::{BlockEntityData, LevelState};

#[link(wasm_import_module = "host")]
extern "C" {
    #[link_name = "entity_removed"]
    fn _entity_removed(a0: u32, a1: u32, a2: u32, a3: u32);
    #[link_name = "entity_iron_ore"]
    fn _entity_iron_ore(a0: u32, a1: u32, a2: u32, a3: u32, x: u32, y: u32, z: u32, qty: u64);
}

fn from_uuid(v: u128) -> [u32; 4] {
    [
        (v & 0xffff_ffff) as u32,
        ((v >> 32) & 0xffff_ffff) as u32,
        ((v >> 64) & 0xffff_ffff) as u32,
        ((v >> 96) & 0xffff_ffff) as u32,
    ]
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

        match &e.data {
            BlockEntityData::IronOre(v) => {
                let [a0, a1, a2, a3] = from_uuid(id.as_u128());
                unsafe {
                    _entity_iron_ore(a0, a1, a2, a3, e.x as _, e.y as _, e.z as _, v.quantity)
                }
            }
        }
    }
}
