//! AIH_BASICCOP.CPP: `AIHigh_BasicCop`, a police car (AI or human): its set-up, the spike belt
//! and road block taken away once out of sight, whether to cut a car off, and a blockade's
//! road flares and barriers put on the road.

use crate::abi::{self, Saved};
use crate::aihigh::{self, VT_BASIC_COP};
use crate::ailife::is_slice_in_any_visible_area;
use crate::aiworld::spline_distance;
use crate::bworldsm;
use crate::fixed::{fixedmult, transpose};
use crate::globals::{AI_ELAPSED_TIME, BWORLDSM_SLICES, FAST_RANDOM, HUMAN_RACE_CARS, I32, RAND_SEED, RAND_TEMP, SIM_COUNTER, SPIKE_BELT, TOP_SPEED_CAP};
use crate::types::{AIHighBasicCop, AIHighPlayer, BWorldSmPos, Blockade, Car, Coord};
use ps1rt::{Cpu, Mem};

/// `AICop_gRoadBlockState`: no road block (0), being set up, up (2).
pub const ROAD_BLOCK_STATE: I32 = I32(0x8013_D864);
/// `Object_customSliceNum`: the slice of the custom objects (a blockade's); `Object_customSFXInst`:
/// their road flares (a count, then 16 bytes each).
pub const CUSTOM_SLICE_NUM: I32 = I32(0x8013_E5B0);
const CUSTOM_SFX_INST: u32 = 0x8013_E5AC;

/// `Car_tObj.carFlags`: a police car of the second type (the stronger).
const CAR_COP_TYPE: i32 = 0x40;

/// `AIHigh_BasicCop::AIHigh_BasicCop(Car_tObj*, int)` (AIH_BASICCOP.CPP, 0x8005c954): police
/// car `index` of its type, no blockade, no spike belt asked for, not driving away.
pub fn construct(c: &mut Cpu, this: AIHighBasicCop, car: Car, index: i32) -> AIHighBasicCop {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1], r.0[2]) = (car.0, index as u32, this.0);
        r.enter(c, 0x8005_bb08, 0x8005_c978, |c| aihigh::base_construct(c, this.ai_high_base(), car));
        let m = &mut c.m;
        aihigh::set_vt(m, this.0, VT_BASIC_COP);
        this.set_cop_index_(m, index);
        let kind = (car.car_flags(m) & CAR_COP_TYPE != 0) as i32;
        this.set_type_(m, kind);
        let b = this.blockade_();
        b.set_mode(m, 0);
        b.set_target(m, AIHighPlayer(0));
        b.set_request_spike_belt_at_slice(m, -1);
        b.set_blockade_speech_flags(m, 0);
        this.set_drive_away_(m, 0);
        this
    })
}

pub fn abi_construct(c: &mut Cpu) {
    c.r[2] = construct(c, AIHighBasicCop(c.r[4]), Car(c.r[5]), c.r[6] as i32).0;
}

/// `AIHigh_BasicCop::CheckSpikeBelt` (AIH_BASICCOP.CPP, 0x8005c9dc): every 5 s a spike belt
/// out of the humans' sight is taken away (still seen, it stays 5 s more); a road block up
/// and out of sight too.
pub fn check_spike_belt(c: &mut Cpu) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1]) = (SPIKE_BELT.0, 0x8011_0000);
        let m = &mut c.m;
        let belt = SPIKE_BELT;
        if belt.active_(m) != 0 && SIM_COUNTER.get(m).wrapping_sub(belt.freshen_time_(m)) >= 320 {
            let slice = belt.slice_(m);
            if r.enter(c, 0x8006_900c, 0x8005_ca30, |c| is_slice_in_any_visible_area(&mut c.m, slice)) != 0 {
                let now = SIM_COUNTER.get(&mut c.m);
                belt.set_freshen_time_(&mut c.m, now);
            } else {
                r.call(c, nfs4rc::f_8007e1c0, &[], 0x8005_ca40); // BWorld_InitSpikeBelt
                belt.set_active_(&mut c.m, 0);
            }
        }
        if ROAD_BLOCK_STATE.get(&mut c.m) == 2 {
            let slice = CUSTOM_SLICE_NUM.get(&mut c.m);
            if r.enter(c, 0x8006_900c, 0x8005_ca78, |c| is_slice_in_any_visible_area(&mut c.m, slice)) == 0 {
                r.call(c, nfs4rc::f_800a61e4, &[], 0x8005_ca88); // Object_ClearCustomObjects
                ROAD_BLOCK_STATE.set(&mut c.m, 0);
            }
        }
    })
}

pub fn abi_check_spike_belt(c: &mut Cpu) {
    check_spike_belt(c);
}

/// One step of the game's quick random numbers (`randtemp` the product).
pub(crate) fn fast_random_step(m: &mut Mem) -> u32 {
    let t = FAST_RANDOM.get(m).wrapping_mul(RAND_SEED.get(m));
    RAND_TEMP.set(m, t);
    FAST_RANDOM.set(m, t & 0xFFFF);
    t
}

/// A quick random number from 0 to 999 (the original's `(randtemp >> 8 & 0xffff) * 1000 >> 16`).
pub(crate) fn per_mille(m: &mut Mem) -> i32 {
    let t = fast_random_step(m);
    ((t >> 8 & 0xFFFF).wrapping_mul(1000) >> 16) as i32
}

/// `AIHigh_BasicCop::ShouldIPerformCutOffBlock` (AIH_BASICCOP.CPP, 0x8005caa4): by chance
/// (`prob` per 32 thousandths a tick, by the time step), when `car` is beside this police car
/// across the road (more than its width plus 1 m apart, within 4 m) and ahead along its way
/// (more than twice its length plus 2 m, within 12 m).
pub fn should_i_perform_cut_off_block(c: &mut Cpu, this: AIHighBasicCop, prob: i32, car: Car) -> bool {
    let m = &mut c.m;
    let chance = (prob / 32).wrapping_mul(AI_ELAPSED_TIME.get(m)).wrapping_mul(1000) / 0x1_0000;
    if per_mille(m) >= chance {
        return false;
    }
    let me = this.ai_high_base().car_obj_(m);
    let across = me.road_position(m).wrapping_sub(car.road_position(m)).wrapping_abs();
    if car.n().dimension().x(m).wrapping_add(0x1_0000) >= across || across > 0x3_FFFF {
        return false;
    }
    let d = abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1]) = (car.0, this.0);
        r.enter(c, 0x8007_3b04, 0x8005_cba8, |c| spline_distance(c, me, car))
    });
    let m = &mut c.m;
    let ahead = d.wrapping_mul(me.direction(m));
    (car.n().dimension().z(m) << 1).wrapping_add(0x2_0000) < ahead && ahead <= 0xB_FFFF
}

pub fn abi_should_i_perform_cut_off_block(c: &mut Cpu) {
    c.r[2] = should_i_perform_cut_off_block(c, AIHighBasicCop(c.r[4]), c.r[5] as i32, Car(c.r[6])) as u32;
}

/// `Blockade_AddRoadFlare` (AIH_BASICCOP.CPP, 0x8005cc04): a road flare (sound effect 22) at
/// `pos`.
pub fn add_road_flare(m: &mut Mem, pos: Coord) {
    let inst = m.r32(CUSTOM_SFX_INST);
    let n = m.r32(inst);
    let e = inst.wrapping_add(4).wrapping_add(n << 4);
    let x = pos.x(m);
    m.w32(e, x as u32);
    let y = pos.y(m);
    m.w32(e.wrapping_add(4), y as u32);
    let z = pos.z(m);
    m.w16(e.wrapping_add(12), 22);
    m.w16(e.wrapping_add(14), 0);
    m.w32(e.wrapping_add(8), z as u32);
    let n = m.r32(inst);
    m.w32(inst, n.wrapping_add(1));
}

pub fn abi_add_road_flare(c: &mut Cpu) {
    add_road_flare(&mut c.m, Coord(c.r[4]));
}

/// `Blockade_AddObject` (AIH_BASICCOP.CPP, 0x8005cc58): a custom object of definition `def` at
/// `pos` on slice `slice`, standing on the road (its rows across, the road's normal and
/// forward). The object is built on the stack, where the fields it doesn't set keep what was
/// there.
pub fn add_object(c: &mut Cpu, slice: i32, pos: Coord, def: i32) {
    abi::frame(c, 272, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1], r.0[2]) = (slice as u32, def as u32, pos.0);
        c.m.w32(sp + 16, 0);
        c.m.w32(sp + 84, 1);
        let id = r.call(c, nfs4rc::f_800a64d0, &[def as u32], 0x8005_cc90); // Object_GetObjDefID
        let m = &mut c.m;
        m.w32(sp + 92, id);
        m.w32(sp + 96, 0x2_3916);
        let p = pos.get(m);
        Coord(sp + 32).set(m, p);
        m.w32(sp + 88, def as u32);
        let at = BWorldSmPos(sp + 112);
        r.0[0] = at.0;
        r.enter(c, 0x8007_f788, 0x8005_cccc, |c| bworldsm::set_slice(c, slice, at));
        r.enter(c, 0x8008_04e8, 0x8005_ccdc, |c| bworldsm::find_closest_quad_rez(c, pos, at, 1));
        let n = r.enter(c, 0x8008_0f44, 0x8005_cce4, |c| bworldsm::u_normal(c, at)).get(&mut c.m);
        Coord(sp + 60).set(&mut c.m, n);
        let f = r.enter(c, 0x8008_0f6c, 0x8005_cd04, |c| bworldsm::u_forward(c, at)).get(&mut c.m);
        let m = &mut c.m;
        Coord(sp + 72).set(m, f);
        let across = [
            fixedmult(n.y, f.z).wrapping_sub(fixedmult(n.z, f.y)),
            fixedmult(n.z, f.x).wrapping_sub(fixedmult(n.x, f.z)),
            fixedmult(n.x, f.y).wrapping_sub(fixedmult(n.y, f.x)),
        ];
        for (k, v) in across.iter().enumerate() {
            m.w32(sp + 48 + 4 * k as u32, *v as u32);
        }
        transpose(m, sp + 48, sp + 48);
        (r.0[0], r.0[1], r.0[2], r.0[3]) = (across[2] as u32, sp + 72, sp + 60, sp + 48);
        r.call(c, nfs4rc::f_800a657c, &[sp + 16, 1], 0x8005_cdb8); // Object_AddCustomObject
    })
}

pub fn abi_add_object(c: &mut Cpu) {
    add_object(c, c.r[4] as i32, Coord(c.r[5]), c.r[6] as i32);
}

/// `PlacePointOnRoad` (AIH_BASICCOP.CPP, 0x8005cdd4): `pos`, across and along slice `slice`
/// (its x and z from the slice's centre in the slice's directions), put on the road's surface
/// there (found from a copy of the first human's road position).
pub fn place_point_on_road(c: &mut Cpu, slice: i32, pos: Coord) {
    abi::frame(c, 208, |c, sp| {
        let mut r = Saved::of(c);
        let m = &mut c.m;
        let p = pos.get(m);
        Coord(sp + 168).set(m, p);
        let human = m.r32(HUMAN_RACE_CARS);
        for w in (0..0x84).step_by(4) {
            let v = m.r32(human + 8 + w);
            m.w32(sp + 32 + w, v);
        }
        m.w16(sp + 32, slice as u16);
        let s = (BWORLDSM_SLICES.get(m) as u32).wrapping_add((slice as u32) << 5);
        let centre = Coord(s).get(m);
        Coord(sp + 16).set(m, centre);
        let b = |m: &mut Mem, k: u32| (m.r8(s + k) as i8 as i32) << 9;
        let (rx, rz, fx, fz) = (b(m, 18), b(m, 20), b(m, 15), b(m, 17));
        let x = centre.x.wrapping_add(fixedmult(rx, p.x)).wrapping_add(fixedmult(fx, p.z));
        pos.set_x(m, x);
        pos.set_y(m, centre.y);
        let along = fixedmult(rz, p.x);
        let z = centre.z.wrapping_add(along).wrapping_add(fixedmult(fz, p.z));
        pos.set_z(m, z);
        let at = BWorldSmPos(sp + 32);
        (r.0[0], r.0[1], r.0[2], r.0[3]) = (at.0, along as u32, rz as u32, pos.0);
        r.enter(c, 0x8008_04e8, 0x8005_cf24, |c| bworldsm::find_closest_quad_rez(c, pos, at, 1));
        let n = r.enter(c, 0x8008_0f44, 0x8005_cf2c, |c| bworldsm::u_normal(c, at));
        let m = &mut c.m;
        let quad = if m.r32(sp + 152) != 0 { sp + 40 } else { (BWORLDSM_SLICES.get(m) as u32).wrapping_add(((m.r16(sp + 32) as i16 as i32) << 5) as u32) };
        let y = r.call(c, nfs4rc::f_800b6de8, &[n.0, quad, pos.0], 0x8005_cf60); // GetPlaneY
        pos.set_y(&mut c.m, y as i32);
    })
}

pub fn abi_place_point_on_road(c: &mut Cpu) {
    place_point_on_road(c, c.r[4] as i32, Coord(c.r[5]));
}

/// `AIHigh_BasicCop::SetupBlockadeElements` (AIH_BASICCOP.CPP, 0x8005cf80): the blockade's
/// custom objects anew on its slice: a wedge of road flares (1), or a line across (2, a gap in
/// the second row), barriers either side (4); the elements then done.
pub fn setup_blockade_elements(c: &mut Cpu, b: Blockade) {
    abi::frame(c, 64, |c, sp| {
        let mut r = Saved::of(c);
        r.0[4] = b.0;
        if b.flags(&mut c.m) == 0 {
            return;
        }
        r.call(c, nfs4rc::f_800a61e4, &[], 0x8005_cfbc); // Object_ClearCustomObjects
        let flags = b.flags(&mut c.m);
        let slice = b.slice(&mut c.m);
        r.0[3] = slice as u32;
        CUSTOM_SLICE_NUM.set(&mut c.m, slice);
        let pt = Coord(sp + 16);
        // A flare at (x, z by the blockade's direction): placed, then added.
        let flare = |c: &mut Cpu, r: &Saved, x: i32, z: i32, ra: u32| {
            let m = &mut c.m;
            m.w32(sp + 16, x as u32);
            m.w32(sp + 20, 0);
            let z = b.direction(m).wrapping_mul(z);
            m.w32(sp + 24, z as u32);
            r.enter(c, 0x8005_cdd4, ra, |c| place_point_on_road(c, slice, pt));
            r.enter(c, 0x8005_cc04, ra + 8, |c| add_road_flare(&mut c.m, pt));
        };
        // (the registers as the original's loops have them at each call and after)
        r.0[0] = 0;
        if flags & 1 != 0 {
            for i in 0..5 {
                (r.0[0], r.0[1]) = ((i + 1) as u32, (-0x10_0000 + 0x4_0000 * i) as u32);
                flare(c, &r, i << 17, -0x10_0000 + 0x4_0000 * i, 0x8005_d00c);
            }
            for k in 1..5 {
                let (x, z) = (-0x2_0000 * k, -0xC_0000 + 0x4_0000 * (k - 1));
                (r.0[0], r.0[1], r.0[2]) = (x as u32, z as u32, (k + 1) as u32);
                flare(c, &r, x, z, 0x8005_d060);
            }
            (r.0[0], r.0[1]) = (-0xA_0000i32 as u32, 0x4_0000);
        } else {
            r.0[0] = 0xFFFA_0000;
            if flags & 2 != 0 {
                for k in 0..7 {
                    let (x, z) = (-0x6_0000 + 0x2_0000 * k, -0x18_0000 + 0x4_0000 * k);
                    (r.0[0], r.0[1], r.0[2]) = (x as u32, z as u32, (k + 1) as u32);
                    if k > 0 {
                        r.0[5] = 3;
                    }
                    flare(c, &r, x, z, 0x8005_d0cc);
                }
                r.0[5] = 3;
                for k in 0..7 {
                    let (x, z) = (0x6_0000 - 0x2_0000 * k, -0x18_0000 + 0x4_0000 * k);
                    (r.0[0], r.0[1], r.0[2]) = (k as u32, x as u32, z as u32);
                    if k != 3 {
                        flare(c, &r, x, z, 0x8005_d128);
                    }
                }
                (r.0[0], r.0[1], r.0[2]) = (7, -0x8_0000i32 as u32, 0x4_0000);
            }
        }
        if b.flags(&mut c.m) & 4 != 0 {
            let def = r.call(c, nfs4rc::f_800a6524, &[3], 0x8005_d16c) as i32; // Object_FindDefWithThisID
            r.0[0] = def as u32;
            if def != -1 {
                for (x, ra) in [(-0x2_8000, 0x8005_d1a4), (0x2_8000, 0x8005_d1e0)] {
                    let m = &mut c.m;
                    m.w32(sp + 16, x as u32);
                    m.w32(sp + 20, 0);
                    let z = (b.direction(m) << 18).wrapping_neg();
                    m.w32(sp + 24, z as u32);
                    r.enter(c, 0x8005_cdd4, ra, |c| place_point_on_road(c, slice, pt));
                    r.enter(c, 0x8005_cc58, ra + 0x10, |c| add_object(c, slice, pt, def));
                }
            }
        }
        b.set_flags(&mut c.m, 0);
    })
}

pub fn abi_setup_blockade_elements(c: &mut Cpu) {
    setup_blockade_elements(c, Blockade(c.r[5]));
}

/// `AIHigh_BasicCop::HandleBlockadeSpeech` (AIH_BASICCOP.CPP, 0x8005d218): with a blockade's
/// speech pending, once its target (still racing) is within 32 slices it notes whether it is
/// before or past the blockade by 28% of its top speed (in slices); both noted, the radio
/// says the road block is up.
pub fn handle_blockade_speech(c: &mut Cpu, this: AIHighBasicCop) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let m = &mut c.m;
        let b = this.blockade_();
        let flags = b.blockade_speech_flags(m);
        if flags == 0 {
            return;
        }
        let target = b.target(m).0;
        let car = Car(m.r32(target));
        if car.0 == 0 || car.blowout(m) != 0 {
            b.set_blockade_speech_flags(m, 0);
            return;
        }
        let gap = car.stats().slice(m).wrapping_sub(b.slice(m));
        if gap.wrapping_abs() >= 33 {
            return;
        }
        let kind = car.render().current_car_type(m) as i32;
        let top = m.r32(TOP_SPEED_CAP.wrapping_add((kind << 2) as u32)) as i32;
        let x = top.wrapping_mul(28);
        let reach = ((((x as i64) * 0x51EB_851F) >> 32) as i32 >> 21).wrapping_sub(x >> 31);
        if gap < reach {
            b.set_blockade_speech_flags(m, flags | 2);
        } else if reach < gap {
            b.set_blockade_speech_flags(m, flags | 4);
        }
        if b.blockade_speech_flags(m) & 6 != 6 {
            return;
        }
        let car = this.ai_high_base().car_obj_(m);
        crate::aih_basicperp::speech(c, &r, car, 7, 0x8005_d300, 0x8005_d31c, &[]);
        b.set_blockade_speech_flags(&mut c.m, 0);
        ROAD_BLOCK_STATE.set(&mut c.m, 2);
    })
}

pub fn abi_handle_blockade_speech(c: &mut Cpu) {
    handle_blockade_speech(c, AIHighBasicCop(c.r[4]));
}

/// `AIHigh_BasicCop::~AIHigh_BasicCop` (AIHIGH.H, 0x8005d33c).
pub fn abi_destruct(c: &mut Cpu) {
    aihigh::destruct_to_base(c, c.r[4], c.r[5], None, 0x8005_d34c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aihigh::tests::{ARGS, HEAP_OBJS, OBJS, args, shuffle};
    use crate::test_util::{SCRATCH_TOP, against_original, same_stack};

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    /// The point to place (local to its slice) and its slice.
    const POINT: u32 = ARGS + 0x40;

    fn slice(c: &mut Cpu) -> u32 {
        c.m.r32(ARGS + 0x4C)
    }

    #[test]
    fn basiccop_fuzz() {
        let cases: [(&str, fn(&mut Cpu), fn(&mut Cpu, [u32; 7]) -> Vec<u32>, Port); 9] = [
            ("Construct", nfs4rc::f_8005c954, |_, a| vec![OBJS + 0x700, a[0], a[2]], |c, a| Some(construct(c, AIHighBasicCop(a[0]), Car(a[1]), a[2] as i32).0)),
            ("CheckSpikeBelt", nfs4rc::f_8005c9dc, |_, a| vec![a[6]], |c, _| {
                check_spike_belt(c);
                None
            }),
            ("ShouldIPerformCutOffBlock", nfs4rc::f_8005caa4, |c, a| vec![a[6], [0, 0x800, 0x8000, 0x10_0000, 0x10_0000, 0x10_0000][a[5] as usize], c.m.r32(ARGS + 0x50)], |c, a| {
                Some(should_i_perform_cut_off_block(c, AIHighBasicCop(a[0]), a[1] as i32, Car(a[2])) as u32)
            }),
            ("AddRoadFlare", nfs4rc::f_8005cc04, |_, a| vec![a[0] + 0xA0], |c, a| {
                add_road_flare(&mut c.m, Coord(a[0]));
                None
            }),
            ("AddObject", nfs4rc::f_8005cc58, |c, a| vec![slice(c), a[0] + 0xA0, a[5] % 3], |c, a| {
                add_object(c, a[0] as i32, Coord(a[1]), a[2] as i32);
                None
            }),
            ("PlacePointOnRoad", nfs4rc::f_8005cdd4, |c, _| vec![slice(c), POINT], |c, a| {
                place_point_on_road(c, a[0] as i32, Coord(a[1]));
                None
            }),
            ("SetupBlockadeElements", nfs4rc::f_8005cf80, |_, a| vec![a[6], a[6] + 0x20], |c, a| {
                setup_blockade_elements(c, Blockade(a[1]));
                None
            }),
            ("HandleBlockadeSpeech", nfs4rc::f_8005d218, |_, a| vec![a[6]], |c, a| {
                handle_blockade_speech(c, AIHighBasicCop(a[0]));
                None
            }),
            ("Destruct", nfs4rc::f_8005d33c, |_, a| vec![HEAP_OBJS[a[3] as usize % 3], a[4] * 3], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_destruct(c);
                None
            }),
        ];
        for (k, (name, original, call, port)) in cases.into_iter().enumerate() {
            against_original(name, 1_000, 0x3C10 + k as u64, shuffle, original, &|c| {
                let a = args(c);
                call(c, a)
            }, port);
        }
    }

    /// The stack the blockades' objects are built on (the fields they don't set come from it).
    #[test]
    fn basiccop_stack_fuzz() {
        let cases: [(&str, u32, fn(&mut Cpu)); 5] = [
            ("CheckSpikeBelt", 0x8005_c9dc, nfs4rc::f_8005c9dc),
            ("ShouldIPerformCutOffBlock", 0x8005_caa4, nfs4rc::f_8005caa4),
            ("SetupBlockadeElements", 0x8005_cf80, nfs4rc::f_8005cf80),
            ("HandleBlockadeSpeech", 0x8005_d218, nfs4rc::f_8005d218),
            ("Construct", 0x8005_c954, nfs4rc::f_8005c954),
        ];
        for (k, (name, addr, original)) in cases.into_iter().enumerate() {
            same_stack(name, addr, original, 0x8006_0000, 0x801F_FF00, 300, 0x3C50 + k as u64, shuffle, &|c, _| {
                let a = args(c);
                match name {
                    "ShouldIPerformCutOffBlock" => vec![a[6], 0x10_0000, a[0]],
                    "SetupBlockadeElements" => vec![a[6], a[6] + 0x20],
                    "Construct" => vec![OBJS + 0x700, a[0], a[2]],
                    _ => vec![a[6]],
                }
            });
        }
        let _ = SCRATCH_TOP;
    }
}
