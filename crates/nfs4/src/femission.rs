//! FEMISSION.CPP: the hot pursuit missions' manager (`tMissionManager`, `missionManager`): their
//! descriptions read from `zHPurs.mis` (or `zHPurs2.mis`): the tiers (4 bytes each: +2 the first
//! mission), the missions (20 bytes each from +32: +8 the first stage, +10 how many) and their
//! stages (44 bytes each from +1312); the mission to race, a mission's stages.
//!
//! Written as FEMENU.CPP is (see there).

use crate::call::{call, with_buffer};
use crate::femenuextended::SPRINTF;
use crate::globals::{FRONT_END, PATHS};
use ps1rt::{Cpu, Mem};

// A mission manager: +0 how many tiers, +4 its definition (the tiers, the missions from +32, the
// stages from +1312; 12576 bytes reserved).
const TIERS: u32 = 0;
const DEFINITION: u32 = 4;
const TIER_SIZE: u32 = 4;
const FIRST_MISSION: u32 = 2;
const MISSIONS: u32 = 32;
const MISSION_SIZE: u32 = 20;
const FIRST_STAGE: u32 = 8;
const STAGE_COUNT: u32 = 10;
const STAGES: u32 = 1312;
const STAGE_SIZE: u32 = 44;
const DEFINITION_SIZE: u32 = 12576;
/// The file: +0 how many tiers, +4 missions, +8 stages; the tiers, missions and stages from +12.
const FILE_DATA: u32 = 12;

/// The mission to race: its tier and the mission in it (`frontEnd + 0x12b`, `+0x12c`).
const MISSION_TIER: u32 = FRONT_END + 0x12B;
const MISSION: u32 = FRONT_END + 0x12C;
/// "%s%s", "zHPurs2.mis", "zHPurs.mis", "Missions"; the front end's directory.
const PATH_FORMAT: u32 = 0x8001_1648;
const FILE2: u32 = 0x8001_1650;
const FILE: u32 = 0x8001_165C;
const MEMORY_NAME: u32 = 0x8001_1668;
const FRONT_END_PATH: u32 = PATHS + 0x94;

/// `loadfileadr`, `reservememadr`, `blockmove`, `purgememadr`.
const LOAD_FILE: fn(&mut Cpu) = nfs4rc::f_800e63d4;
const RESERVE: fn(&mut Cpu) = nfs4rc::f_800e5f28;
const BLOCKMOVE: fn(&mut Cpu) = nfs4rc::f_800e6ec8;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

/// Mission `k` of the definition.
fn mission(definition: u32, k: u32) -> u32 {
    definition.wrapping_add(k.wrapping_mul(MISSION_SIZE)) + MISSIONS
}

/// Tier `tier`'s first mission.
fn first_mission(m: &mut Mem, definition: u32, tier: u32) -> u32 {
    m.r8(definition.wrapping_add(tier.wrapping_mul(TIER_SIZE)) + FIRST_MISSION) as u32
}

/// `tMissionManager::Initialize` (FEMISSION.CPP, 0x80034198): no tiers, no definition; the
/// first mission of the first tier to race.
pub fn initialize(m: &mut Mem, this: u32) {
    m.w8(this + TIERS, 0);
    m.w32(this + DEFINITION, 0);
    m.w8(MISSION_TIER, 0);
    m.w8(MISSION, 0);
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(&mut c.m, c.r[4]);
}

/// `tMissionManager::LoadDescription` (FEMISSION.CPP, 0x800341b4): the missions' file (the
/// second one's for `second`) read; its tiers, missions and stages copied into the definition
/// (memory reserved for it the first time); the file freed.
pub fn load_description(c: &mut Cpu, this: u32, second: bool) {
    let data = with_buffer(c, 112, |c, name| {
        let path = c.m.r32(FRONT_END_PATH);
        call(c, SPRINTF, &[name, PATH_FORMAT, path, if second { FILE2 } else { FILE }]);
        call(c, LOAD_FILE, &[name, 16])
    });
    let m = &mut c.m;
    let tiers = m.r8(data);
    m.w8(this + TIERS, tiers);
    let (missions, stages) = (m.r32(data + 4), m.r32(data + 8));
    if m.r32(this + DEFINITION) == 0 {
        let definition = call(c, RESERVE, &[MEMORY_NAME, DEFINITION_SIZE, 0]);
        c.m.w32(this + DEFINITION, definition);
    }
    let mut from = data + FILE_DATA;
    let (definition, tiers) = (c.m.r32(this + DEFINITION), c.m.r8(this + TIERS) as u32);
    call(c, BLOCKMOVE, &[from, definition, tiers * TIER_SIZE]);
    let size = missions.wrapping_mul(MISSION_SIZE);
    let (definition, tiers) = (c.m.r32(this + DEFINITION), c.m.r8(this + TIERS) as u32);
    from = from.wrapping_add(tiers * TIER_SIZE);
    call(c, BLOCKMOVE, &[from, definition + MISSIONS, size]);
    let definition = c.m.r32(this + DEFINITION);
    call(c, BLOCKMOVE, &[from.wrapping_add(size), definition + STAGES, stages.wrapping_mul(STAGE_SIZE)]);
    call(c, PURGE, &[data]);
}

pub fn abi_load_description(c: &mut Cpu) {
    load_description(c, c.r[4], c.r[5] != 0);
}

/// `tMissionManager::ReleaseDescription` (FEMISSION.CPP, 0x800342e8): the definition freed.
pub fn release_description(c: &mut Cpu, this: u32) {
    let definition = c.m.r32(this + DEFINITION);
    if definition != 0 {
        call(c, PURGE, &[definition]);
        c.m.w32(this + DEFINITION, 0);
    }
}

pub fn abi_release_description(c: &mut Cpu) {
    release_description(c, c.r[4]);
}

/// `tMissionManager::GetMissionStages` (FEMISSION.CPP, 0x80034324): mission `k` of tier `tier`
/// (`short`s): its first stage (written at `out`) and how many.
pub fn get_mission_stages(m: &mut Mem, this: u32, tier: u32, k: u32, out: u32) -> u32 {
    let definition = m.r32(this + DEFINITION);
    let first = first_mission(m, definition, sx(tier));
    let mi = mission(definition, first.wrapping_add(sx(k)));
    let stage = m.r16(mi + FIRST_STAGE) as u32;
    m.w32(out, definition.wrapping_add(stage.wrapping_mul(STAGE_SIZE)) + STAGES);
    m.r8(mi + STAGE_COUNT) as u32
}

pub fn abi_get_mission_stages(c: &mut Cpu) {
    c.r[2] = get_mission_stages(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tMissionManager::GetMissionToRace` (FEMISSION.CPP, 0x8003438c): the front end's mission,
/// written at `out`.
pub fn get_mission_to_race(m: &mut Mem, this: u32, out: u32) {
    let definition = m.r32(this + DEFINITION);
    let tier = m.r8(MISSION_TIER) as u32;
    let k = first_mission(m, definition, tier).wrapping_add(m.r8(MISSION) as u32);
    m.w32(out, mission(definition, k));
}

pub fn abi_get_mission_to_race(c: &mut Cpu) {
    get_mission_to_race(&mut c.m, c.r[4], c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// A mission manager and its definition (4 tiers, 16 missions, 32 stages); an output.
    const MANAGER: u32 = 0x8014_7000;
    const DEF: u32 = 0x8014_7400;
    const OUT: u32 = 0x8014_7010;

    /// FEMENU's structures, then the manager over a definition of random bytes (each tier's
    /// first mission and each mission's first stage in range), the mission to race.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        m.w8(MANAGER + TIERS, 4);
        m.w32(MANAGER + DEFINITION, DEF);
        for k in 0..(STAGES + 32 * STAGE_SIZE) {
            m.w8(DEF + k, rng.next() as u8);
        }
        for t in 0..4 {
            m.w8(DEF + 4 * t + 2, pick(rng, 12) as u8);
        }
        for k in 0..16 {
            m.w16(mission(DEF, k) + 8, pick(rng, 32) as u16);
        }
        m.w8(MISSION_TIER, pick(rng, 4) as u8);
        m.w8(MISSION, pick(rng, 4) as u8);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tMissionManager::Initialize", 0x8003_4198, nfs4rc::f_80034198, |_, _| vec![MANAGER]),
            ("tMissionManager::ReleaseDescription", 0x8003_42e8, nfs4rc::f_800342e8, |c, r| {
                // (a definition of the heap's now and then, to free)
                if pick(r, 2) == 0 {
                    let d = crate::abi::call(c, RESERVE, &[MEMORY_NAME, 64, 0]);
                    c.m.w32(MANAGER + DEFINITION, d);
                } else if pick(r, 2) == 0 {
                    c.m.w32(MANAGER + DEFINITION, 0);
                }
                vec![MANAGER]
            }),
            ("tMissionManager::GetMissionStages", 0x8003_4324, nfs4rc::f_80034324, |_, r| vec![MANAGER, pick(r, 4), pick(r, 4), OUT]),
            ("tMissionManager::GetMissionToRace", 0x8003_438c, nfs4rc::f_8003438c, |_, _| vec![MANAGER, OUT]),
        ]
    }

    #[test]
    fn femission_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x3155 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x3156 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
