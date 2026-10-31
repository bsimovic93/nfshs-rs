//! FEMISSION.CPP: the hot pursuit missions' manager (`tMissionManager`, `missionManager`): their
//! descriptions read from `zHPurs.mis` (or `zHPurs2.mis`): the tiers (4 bytes each: +2 the first
//! mission), the missions (20 bytes each from +32: +8 the first stage, +10 how many) and their
//! stages (44 bytes each from +1312); the mission to race, a mission's stages.

use crate::abi::{self, Saved};
use crate::femenuextended::SPRINTF;
use crate::globals::{FRONT_END, PATHS};
use ps1rt::Cpu;

// A mission manager: +0 how many tiers, +4 its definition.
const TIERS: u32 = 0;
const DEFINITION: u32 = 4;
const MISSIONS: u32 = 32;
const MISSION_SIZE: u32 = 20;
const STAGES: u32 = 1312;
const STAGE_SIZE: u32 = 44;

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

/// Mission `k` of the definition.
fn mission(definition: u32, k: u32) -> u32 {
    definition.wrapping_add(k.wrapping_mul(MISSION_SIZE)) + MISSIONS
}

/// `tMissionManager::Initialize` (FEMISSION.CPP, 0x80034198): no tiers, no definition; the
/// first mission of the first tier to race.
pub fn abi_initialize(c: &mut Cpu) {
    let this = c.r[4];
    let m = &mut c.m;
    m.w8(this + TIERS, 0);
    m.w32(this + DEFINITION, 0);
    m.w8(MISSION_TIER, 0);
    m.w8(MISSION, 0);
}

/// `tMissionManager::LoadDescription` (FEMISSION.CPP, 0x800341b4): the missions' file (the
/// second one's for `second`) read; its tiers, missions and stages copied into the definition
/// (memory reserved for it the first time); the file freed.
pub fn abi_load_description(c: &mut Cpu) {
    let (this, second) = (c.r[4], c.r[5]);
    abi::frame(c, 128, |c, sp| {
        let mut r = Saved::of(c);
        r.0[4] = this;
        let file = if second != 0 { FILE2 } else { FILE };
        let path = c.m.r32(FRONT_END_PATH);
        let name = sp + 16;
        r.call_leaving(c, SPRINTF, &[name, PATH_FORMAT, path, file], 0x8003_4210);
        let data = r.call_leaving(c, LOAD_FILE, &[name, 16], 0x8003_421c);
        r.0[2] = data;
        let m = &mut c.m;
        let tiers = m.r8(data);
        m.w8(this + TIERS, tiers);
        let missions = m.r32(data + 4);
        r.0[5] = missions;
        let stages = m.r32(data + 8);
        r.0[3] = stages;
        r.0[1] = data + 12;
        if m.r32(this + DEFINITION) == 0 {
            let definition = r.call_leaving(c, RESERVE, &[MEMORY_NAME, 12576, 0], 0x8003_4258);
            c.m.w32(this + DEFINITION, definition);
        }
        let m = &mut c.m;
        let (definition, tiers) = (m.r32(this + DEFINITION), m.r8(this + TIERS) as u32);
        r.call_leaving(c, BLOCKMOVE, &[r.0[1], definition, tiers << 2], 0x8003_426c);
        let size = missions.wrapping_mul(MISSION_SIZE);
        r.0[0] = size;
        let m = &mut c.m;
        let (definition, tiers) = (m.r32(this + DEFINITION), m.r8(this + TIERS) as u32);
        r.0[1] = r.0[1].wrapping_add(tiers << 2);
        r.call_leaving(c, BLOCKMOVE, &[r.0[1], definition + MISSIONS, size], 0x8003_4298);
        let definition = c.m.r32(this + DEFINITION);
        r.call_leaving(c, BLOCKMOVE, &[r.0[1].wrapping_add(size), definition + STAGES, stages.wrapping_mul(STAGE_SIZE)], 0x8003_42bc);
        r.call_leaving(c, PURGE, &[data], 0x8003_42c4);
    })
}

/// `tMissionManager::ReleaseDescription` (FEMISSION.CPP, 0x800342e8): the definition freed.
pub fn abi_release_description(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let definition = c.m.r32(this + DEFINITION);
        if definition != 0 {
            r.call_leaving(c, PURGE, &[definition], 0x8003_4310);
            c.m.w32(this + DEFINITION, 0);
        }
    })
}

/// `tMissionManager::GetMissionStages` (FEMISSION.CPP, 0x80034324): mission `k` of tier `tier`:
/// its first stage, how many.
pub fn abi_get_mission_stages(c: &mut Cpu) {
    let (this, tier, k, out) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let m = &mut c.m;
    let definition = m.r32(this + DEFINITION);
    let first = m.r8(definition.wrapping_add((tier as i16 as i32 as u32) << 2) + 2) as u32;
    let mi = mission(definition, first.wrapping_add(k as i16 as i32 as u32));
    let stage = m.r16(mi + 8) as u32;
    m.w32(out, definition.wrapping_add(stage.wrapping_mul(STAGE_SIZE)) + STAGES);
    c.r[2] = m.r8(mi + 10) as u32;
}

/// `tMissionManager::GetMissionToRace` (FEMISSION.CPP, 0x8003438c): the front end's mission.
pub fn abi_get_mission_to_race(c: &mut Cpu) {
    let (this, out) = (c.r[4], c.r[5]);
    let m = &mut c.m;
    let definition = m.r32(this + DEFINITION);
    let tier = m.r8(MISSION_TIER) as u32;
    let first = m.r8(definition.wrapping_add(tier << 2) + 2) as u32;
    let k = first.wrapping_add(m.r8(MISSION) as u32);
    let mi = mission(definition, k);
    m.w32(out, mi);
    c.r[2] = k.wrapping_mul(MISSION_SIZE) + MISSIONS;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, TOP, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

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

    #[test]
    fn femission_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0x3166 + k as u64, shuffle, &args);
        }
    }
}
