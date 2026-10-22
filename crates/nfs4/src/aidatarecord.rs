//! AIDATARECORD.CPP: the AI's data tables, each a record loaded from a file (or kept in a buffer
//! given to it): a car's acceleration by speed (scaled for the car), the track's curvature and
//! best line by slice, a car's speed for each curvature (scaled by its upgrades). The records
//! are C++ objects: each registers itself in `recordCollection`, and is set up and destroyed
//! through its virtual functions.

use crate::abi::{self, Saved};
use crate::fixed::fixedmult;
use crate::globals::{
    AIDATARECORD_BEST_LINE, AIDATARECORD_RECORD_METHOD, AIDATARECORD_TRACK_CURVE, AIDATARECORD_WHICH_RECORD, GAME_SETUP, HUMAN_RACE_CARS,
    NUM_SLICES, PATHS, RECORD_COLLECTION,
};
use crate::types::AIDataRecord;
use ps1rt::{Cpu, Mem};

/// The records' slots in `recordCollection`.
const RECORDS: u32 = 24;
/// The virtual function table pointer (`.vf`, after the fields) and an acceleration table's
/// scale (`AIDataRecord_AccTable_t.scale_`).
const VF: u32 = 0x54;
const SCALE: u32 = 0x58;
/// Each class's virtual function table.
const VF_RECORD: u32 = 0x8005_5C7C;
const VF_ACC_TABLE: u32 = 0x8005_5C64;
const VF_BEST_LINE: u32 = 0x8005_5C4C;
const VF_TRACK_CURVE: u32 = 0x8005_5C34;
const VF_CURVE_SPEED_TABLE: u32 = 0x8005_5C1C;
/// The size of a record object (`operator new`).
const RECORD_SIZE: u32 = 88;

/// A record's name (its file's), at +8.
fn name(r: AIDataRecord) -> u32 {
    r.0.wrapping_add(8)
}

/// The virtual function at slot `k` (entries of 8 bytes: the `this` adjustment, then the
/// function, from +8): the object to call it on and the function.
pub(crate) fn virtual_fn(m: &mut Mem, obj: u32, k: u32) -> (u32, u32) {
    let vf = m.r32(obj.wrapping_add(VF));
    let adjust = m.r16(vf.wrapping_add(8 * k)) as i16 as i32;
    (obj.wrapping_add(adjust as u32), m.r32(vf.wrapping_add(8 * k + 4)))
}

/// `AIDataRecord_t::AIDataRecord_t(AIDataRecord_WhichRecord_t, char*)` (AIDATARECORD.CPP,
/// 0x8006d624): a record registered, using `buffer` (if any) for its data, to be loaded.
pub fn record_construct(c: &mut Cpu, this: AIDataRecord, buffer: u32) -> AIDataRecord {
    let m = &mut c.m;
    m.w32(this.0.wrapping_add(VF), VF_RECORD);
    add_record_to_collection(m, this);
    this.set_pre_allocated_buffer_(m, buffer);
    this.set_record_method_(m, 0);
    this
}

pub fn abi_record_construct(c: &mut Cpu) {
    c.r[2] = record_construct(c, AIDataRecord(c.r[4]), c.r[6]).0;
}

/// `AIDataRecord_t::~AIDataRecord_t` (AIDATARECORD.CPP, 0x8006d66c): its data freed, the record
/// unregistered (and deleted with `flags` & 1).
pub fn record_destruct(c: &mut Cpu, this: AIDataRecord, flags: i32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = flags as u32;
        r.0[1] = this.0;
        c.m.w32(this.0.wrapping_add(VF), VF_RECORD);
        r.enter(c, 0x8006_d920, 0x8006_d694, |c| save_and_purge(c, this));
        remove_record_from_collection(&mut c.m, this);
        r.0[0] = (flags & 1) as u32;
        if flags & 1 != 0 {
            r.call(c, nfs4rc::f_800a4910, &[this.0], 0x8006_d6b0); // operator delete
        }
    })
}

pub fn abi_record_destruct(c: &mut Cpu) {
    record_destruct(c, AIDataRecord(c.r[4]), c.r[5] as i32);
}

/// `AIDataRecord_t::AddRecordToCollection` (AIDATARECORD.CPP, 0x8006d6c4): the record in the
/// first free slot (none free: 0).
pub fn add_record_to_collection(m: &mut Mem, this: AIDataRecord) -> bool {
    for i in 0..RECORDS {
        if m.r32(RECORD_COLLECTION + 4 * i) == 0 {
            m.w32(RECORD_COLLECTION + 4 * i, this.0);
            return true;
        }
    }
    false
}

pub fn abi_add_record_to_collection(c: &mut Cpu) {
    c.r[2] = add_record_to_collection(&mut c.m, AIDataRecord(c.r[4])) as u32;
}

/// `AIDataRecord_t::RemoveRecordFromCollection` (AIDATARECORD.CPP, 0x8006d700).
pub fn remove_record_from_collection(m: &mut Mem, this: AIDataRecord) -> bool {
    for i in 0..RECORDS {
        if m.r32(RECORD_COLLECTION + 4 * i) == this.0 {
            m.w32(RECORD_COLLECTION + 4 * i, 0);
            return true;
        }
    }
    false
}

pub fn abi_remove_record_from_collection(c: &mut Cpu) {
    c.r[2] = remove_record_from_collection(&mut c.m, AIDataRecord(c.r[4])) as u32;
}

/// `AIDataRecord_t::StartUp1` (AIDATARECORD.CPP, 0x8006d73c): no records; the track's best line
/// and curvature made (record kinds 5 and 6).
pub fn start_up1(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        AIDATARECORD_WHICH_RECORD.set(m, 0);
        AIDATARECORD_RECORD_METHOD.set(m, 0);
        for i in (0..RECORDS).rev() {
            m.w32(RECORD_COLLECTION + 4 * i, 0);
        }
        let r = Saved::of(c);
        let p = AIDataRecord(r.call(c, nfs4rc::f_800a48c0, &[RECORD_SIZE], 0x8006_d774)); // operator new
        let line = r.enter(c, 0x8006_da78, 0x8006_d780, |c| best_line_construct(c, p, 5));
        AIDATARECORD_BEST_LINE.set(&mut c.m, line.0);
        let p = AIDataRecord(r.call(c, nfs4rc::f_800a48c0, &[RECORD_SIZE], 0x8006_d78c));
        let curve = r.enter(c, 0x8006_db0c, 0x8006_d798, |c| track_curve_construct(c, p, 6));
        AIDATARECORD_TRACK_CURVE.set(&mut c.m, curve.0);
    })
}

pub fn abi_start_up1(c: &mut Cpu) {
    start_up1(c);
}

/// `AIDataRecord_t::StartUp2` (AIDATARECORD.CPP, 0x8006d7a8): every record set up (its virtual
/// `Setup`).
pub fn start_up2(c: &mut Cpu) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        for i in 0..RECORDS {
            r.0[0] = RECORD_COLLECTION + 4 * i;
            r.0[1] = i;
            let rec = c.m.r32(RECORD_COLLECTION + 4 * i);
            if rec != 0 {
                let (obj, f) = virtual_fn(&mut c.m, rec, 2);
                r.call_ptr(c, f, &[obj], 0x8006_d7f0);
            }
        }
    })
}

pub fn abi_start_up2(c: &mut Cpu) {
    start_up2(c);
}

/// `AIDataRecord_t::CleanUp1` (AIDATARECORD.CPP, 0x8006d814): the best line and the curvature
/// deleted (their virtual destructors).
pub fn clean_up1(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        for (global, ra) in [(AIDATARECORD_BEST_LINE, 0x8006_d840), (AIDATARECORD_TRACK_CURVE, 0x8006_d870)] {
            let rec = global.get(&mut c.m);
            if rec != 0 {
                let (obj, f) = virtual_fn(&mut c.m, rec, 1);
                r.call_ptr(c, f, &[obj, 3], ra);
                global.set(&mut c.m, 0);
            }
        }
    })
}

pub fn abi_clean_up1(c: &mut Cpu) {
    clean_up1(c);
}

/// `AIDataRecord_t::CleanUp2` (AIDATARECORD.CPP, 0x8006d884): nothing.
pub fn abi_clean_up2(_: &mut Cpu) {}

/// `AIDataRecord_t::Setup` (AIDATARECORD.CPP, 0x8006d88c): the record's data: its buffer, else
/// loaded, else room reserved for it.
pub fn setup(c: &mut Cpu, this: AIDataRecord) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let pre = this.pre_allocated_buffer_(&mut c.m);
        if pre != 0 {
            this.set_data_buffer_(&mut c.m, pre);
            return;
        }
        if r.enter(c, 0x8006_d8dc, 0x8006_d8b4, |c| load(c, this)) {
            return;
        }
        let size = this.b_size_(&mut c.m);
        let p = r.call(c, nfs4rc::f_800e5f28, &[name(this), size as u32, 0], 0x8006_d8c8); // reservememadr
        this.set_data_buffer_(&mut c.m, p);
    })
}

pub fn abi_setup(c: &mut Cpu) {
    setup(c, AIDataRecord(c.r[4]));
}

/// `AIDataRecord_t::Load` (AIDATARECORD.CPP, 0x8006d8dc): the record's file loaded (when it is
/// to be read, not recorded); whether it has data.
pub fn load(c: &mut Cpu, this: AIDataRecord) -> bool {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        if this.record_method_(&mut c.m) == 0 {
            let p = r.call(c, nfs4rc::f_800e6850, &[name(this), 0], 0x8006_d904); // loadpackadrz
            this.set_data_buffer_(&mut c.m, p);
        }
        this.data_buffer_(&mut c.m) != 0
    })
}

pub fn abi_load(c: &mut Cpu) {
    c.r[2] = load(c, AIDataRecord(c.r[4])) as u32;
}

/// `AIDataRecord_t::SaveAndPurge` (AIDATARECORD.CPP, 0x8006d920): the record's data dropped
/// (freed unless it was given its buffer); whether it had any.
pub fn save_and_purge(c: &mut Cpu, this: AIDataRecord) -> bool {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let data = this.data_buffer_(&mut c.m);
        if data == 0 {
            return false;
        }
        if this.pre_allocated_buffer_(&mut c.m) == 0 {
            r.call(c, nfs4rc::f_800e612c, &[data], 0x8006_d958); // purgememadr
        }
        this.set_data_buffer_(&mut c.m, 0);
        true
    })
}

pub fn abi_save_and_purge(c: &mut Cpu) {
    c.r[2] = save_and_purge(c, AIDataRecord(c.r[4])) as u32;
}

/// `AIDataRecord_AccTable_t::AIDataRecord_AccTable_t(char*, int, AIDataRecord_WhichRecord_t)`
/// (AIDATARECORD.CPP, 0x8006d970): a car's acceleration table (112 speeds), to be scaled by
/// `scale`, named after the human's car.
pub fn acc_table_construct(c: &mut Cpu, this: AIDataRecord, buffer: u32, scale: i32) -> AIDataRecord {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        r.0[1] = scale as u32;
        r.enter(c, 0x8006_d624, 0x8006_d998, |c| record_construct(c, this, buffer));
        let m = &mut c.m;
        m.w32(this.0.wrapping_add(VF), VF_ACC_TABLE);
        this.set_num_elements_(m, 112);
        this.set_b_size_(m, 224);
        m.w32(this.0.wrapping_add(SCALE), scale as u32);
        let car = m.r32(HUMAN_RACE_CARS);
        let dir = m.r32(PATHS + 0x14);
        r.call(c, nfs4rc::f_800e50a8, &[name(this), 0x8005_5B88, dir, car.wrapping_add(0x240)], 0x8006_d9dc); // sprintf("%s%s.acc")
        this
    })
}

pub fn abi_acc_table_construct(c: &mut Cpu) {
    c.r[2] = acc_table_construct(c, AIDataRecord(c.r[4]), c.r[5], c.r[6] as i32).0;
}

/// `AIDataRecord_AccTable_t::Get` (AIDATARECORD.CPP, 0x8006d9f4): the acceleration at `i` m/s
/// (16.16).
pub fn acc_table_get(m: &mut Mem, this: AIDataRecord, i: i32) -> i32 {
    let at = this.data_buffer_(m).wrapping_add((i as u32).wrapping_mul(2));
    (m.r16(at) as i16 as i32) << 8
}

pub fn abi_acc_table_get(c: &mut Cpu) {
    c.r[2] = acc_table_get(&mut c.m, AIDataRecord(c.r[4]), c.r[5] as i32) as u32;
}

/// `AIDataRecord_AccTable_t::Setup` (AIDATARECORD.CPP, 0x8006da0c): the table loaded, then
/// scaled.
pub fn acc_table_setup(c: &mut Cpu, this: AIDataRecord) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this.0;
        r.enter(c, 0x8006_d88c, 0x8006_da24, |c| setup(c, this));
        let m = &mut c.m;
        for i in 0..112 {
            let v = fixedmult(acc_table_get(m, this, i), m.r32(this.0.wrapping_add(SCALE)) as i32);
            let at = this.data_buffer_(m).wrapping_add(2 * i as u32);
            m.w16(at, (v >> 8) as u16);
        }
    })
}

pub fn abi_acc_table_setup(c: &mut Cpu) {
    acc_table_setup(c, AIDataRecord(c.r[4]));
}

/// A track record's name: `%sTr%02d` and the extension, from the directory `Paths` keeps at
/// `read` (to be read) or `recorded`.
fn track_record_name(c: &mut Cpu, r: &Saved, this: AIDataRecord, read: (u32, u32), recorded: (u32, u32), ra: u32) {
    let m = &mut c.m;
    let (dir, fmt) = if this.record_method_(m) == 0 { read } else { recorded };
    let (dir, track) = (m.r32(PATHS + dir), GAME_SETUP.track(m));
    r.call(c, nfs4rc::f_800e50a8, &[name(this), fmt, dir, track as u32], ra); // sprintf
}

/// `AIDataRecord_BestLine_t::AIDataRecord_BestLine_t` (AIDATARECORD.CPP, 0x8006da78): the
/// track's best line (a byte per slice).
pub fn best_line_construct(c: &mut Cpu, this: AIDataRecord, which: i32) -> AIDataRecord {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        r.enter(c, 0x8006_d624, 0x8006_da90, |c| record_construct(c, this, 0));
        let _ = which;
        let m = &mut c.m;
        let n = NUM_SLICES.get(m);
        m.w32(this.0.wrapping_add(VF), VF_BEST_LINE);
        this.set_num_elements_(m, n);
        this.set_b_size_(m, n);
        track_record_name(c, &r, this, (0x30, 0x8005_5B94), (0x34, 0x8005_5BA4), 0x8006_daf8);
        this
    })
}

pub fn abi_best_line_construct(c: &mut Cpu) {
    c.r[2] = best_line_construct(c, AIDataRecord(c.r[4]), c.r[5] as i32).0;
}

/// `AIDataRecord_TrackCurve_t::AIDataRecord_TrackCurve_t` (AIDATARECORD.CPP, 0x8006db0c): the
/// track's curvature (a byte per slice).
pub fn track_curve_construct(c: &mut Cpu, this: AIDataRecord, which: i32) -> AIDataRecord {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        r.enter(c, 0x8006_d624, 0x8006_db24, |c| record_construct(c, this, 0));
        let _ = which;
        let m = &mut c.m;
        let n = NUM_SLICES.get(m);
        m.w32(this.0.wrapping_add(VF), VF_TRACK_CURVE);
        this.set_num_elements_(m, n);
        this.set_b_size_(m, n.wrapping_add(1));
        track_record_name(c, &r, this, (0x28, 0x8005_5BB4), (0x2C, 0x8005_5BC4), 0x8006_db90);
        this
    })
}

pub fn abi_track_curve_construct(c: &mut Cpu) {
    c.r[2] = track_curve_construct(c, AIDataRecord(c.r[4]), c.r[5] as i32).0;
}

/// `AIDataRecord_TrackCurve_t::Get` (AIDATARECORD.CPP, 0x8006dba4): the curvature at a slice.
pub fn track_curve_get(m: &mut Mem, this: AIDataRecord, slice: i32) -> i32 {
    let data = this.data_buffer_(m);
    m.r8(data.wrapping_add(slice as u32)) as i32
}

pub fn abi_track_curve_get(c: &mut Cpu) {
    c.r[2] = track_curve_get(&mut c.m, AIDataRecord(c.r[4]), c.r[5] as i32) as u32;
}

/// `AIDataRecord_CurveSpeedTable_t::AIDataRecord_CurveSpeedTable_t(char*, ...)`
/// (AIDATARECORD.CPP, 0x8006dbbc): a car's speed for each curvature (256), named `car`.
pub fn curve_speed_table_construct(c: &mut Cpu, this: AIDataRecord, car: u32, which: i32) -> AIDataRecord {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        r.0[1] = car;
        let _ = which;
        r.enter(c, 0x8006_d624, 0x8006_dbe0, |c| record_construct(c, this, 0));
        let m = &mut c.m;
        let method = this.record_method_(m);
        m.w32(this.0.wrapping_add(VF), VF_CURVE_SPEED_TABLE);
        this.set_num_elements_(m, 256);
        this.set_b_size_(m, 256);
        let (dir, fmt) = if method == 0 { (0x10, 0x8005_5BD4) } else { (0x14, 0x8005_5BE0) };
        let dir = m.r32(PATHS + dir);
        r.call(c, nfs4rc::f_800e50a8, &[name(this), fmt, dir, car], 0x8006_dc34); // sprintf("%s%s.qcs")
        this
    })
}

pub fn abi_curve_speed_table_construct(c: &mut Cpu) {
    c.r[2] = curve_speed_table_construct(c, AIDataRecord(c.r[4]), c.r[5], c.r[6] as i32).0;
}

/// `AIDataRecord_CurveSpeedTable_t::Get` (AIDATARECORD.CPP, 0x8006dc4c): the speed for a
/// curvature (its size, at most 255; m/s, 16.16).
pub fn curve_speed_table_get(m: &mut Mem, this: AIDataRecord, curve: i32) -> i32 {
    let i = curve.wrapping_abs();
    let i = if i < 256 { i } else { 255 };
    let data = this.data_buffer_(m);
    (m.r8(data.wrapping_add(i as u32)) as i32) << 16
}

pub fn abi_curve_speed_table_get(c: &mut Cpu) {
    c.r[2] = curve_speed_table_get(&mut c.m, AIDataRecord(c.r[4]), c.r[5] as i32) as u32;
}

/// `AIDataRecord_CurveSpeedTable_t::Upgrade` (AIDATARECORD.CPP, 0x8006dc80): every speed scaled
/// by `mult`.
pub fn curve_speed_table_upgrade(m: &mut Mem, this: AIDataRecord, mult: i32) {
    let mut i = 0;
    while i < this.num_elements_(m) {
        let v = fixedmult(curve_speed_table_get(m, this, i), mult);
        let at = this.data_buffer_(m).wrapping_add(i as u32);
        m.w8(at, (v / 0x1_0000) as u8);
        i += 1;
    }
}

pub fn abi_curve_speed_table_upgrade(c: &mut Cpu) {
    curve_speed_table_upgrade(&mut c.m, AIDataRecord(c.r[4]), c.r[5] as i32);
}

/// `AIDataRecord_CarTracking_t::Get` (AIDATARECORD.CPP, 0x8006dd0c): nothing recorded (0).
pub fn abi_car_tracking_get(c: &mut Cpu) {
    c.r[2] = 0;
}

/// The derived records' destructors (AIDATARECORD.CPP, 0x8006dd14, 0x8006dd34, 0x8006dd54,
/// 0x8006dd74, 0x8006dd94: car tracking, curve speeds, curvature, best line, acceleration): the
/// record's, from the original's frame (returning to `ra`).
fn derived_destruct(c: &mut Cpu, ra: u32) {
    let (this, flags) = (AIDataRecord(c.r[4]), c.r[5] as i32);
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        r.enter(c, 0x8006_d66c, ra, |c| record_destruct(c, this, flags));
    })
}

pub fn abi_car_tracking_destruct(c: &mut Cpu) {
    derived_destruct(c, 0x8006_dd24);
}

pub fn abi_curve_speed_table_destruct(c: &mut Cpu) {
    derived_destruct(c, 0x8006_dd44);
}

pub fn abi_track_curve_destruct(c: &mut Cpu) {
    derived_destruct(c, 0x8006_dd64);
}

pub fn abi_best_line_destruct(c: &mut Cpu) {
    derived_destruct(c, 0x8006_dd84);
}

pub fn abi_acc_table_destruct(c: &mut Cpu) {
    derived_destruct(c, 0x8006_dda4);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{Rng, SCRATCH, against_original};
    use crate::types::Car;

    /// The AI physics' shuffled car (at `SCRATCH`); its records and the track's (at
    /// `SCRATCH+4..12`: acceleration, curve speeds, curvature) given a buffer or not, to be
    /// read or recorded; the collection with gaps; an index, a scale and a car's name
    /// (`SCRATCH+16..24`).
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        crate::aiphysic::tests::shuffle(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let car = Car(m.r32(SCRATCH));
        let records = [car.acceleration_record(m), car.curve_speed_table(m), AIDATARECORD_TRACK_CURVE.get(m)];
        for (k, &rec) in records.iter().enumerate() {
            m.w32(SCRATCH + 4 + 4 * k as u32, rec);
            let r = AIDataRecord(rec);
            if pick(3) == 0 {
                let data = r.data_buffer_(m);
                r.set_pre_allocated_buffer_(m, [0, data][pick(2) as usize]);
            }
            r.set_record_method_(m, [0, 0, 1][pick(3) as usize]);
            if pick(4) == 0 {
                r.set_data_buffer_(m, 0);
            }
        }
        for i in 0..RECORDS {
            if pick(4) == 0 {
                m.w32(RECORD_COLLECTION + 4 * i, 0);
            }
        }
        m.w32(SCRATCH + 16, [pick(300) - 20, pick(112), pick(0x1000) - 0x800][pick(3) as usize] as u32);
        m.w32(SCRATCH + 20, (0x8000 + pick(0x1_0000)) as u32);
        // (a car's name)
        m.w32(SCRATCH + 24, car.0 + 0x240);
    }

    fn arg(c: &mut Cpu, k: u32) -> u32 {
        c.m.r32(SCRATCH + 4 * k)
    }

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    #[test]
    fn aidatarecord_fuzz() {
        let cases: [(&str, fn(&mut Cpu), &[u32], Port); 20] = [
            ("AccTable_t::Get", nfs4rc::f_8006d9f4, &[1, 4], |c, a| Some(acc_table_get(&mut c.m, AIDataRecord(a[0]), a[1] as i32) as u32)),
            ("TrackCurve_t::Get", nfs4rc::f_8006dba4, &[3, 4], |c, a| Some(track_curve_get(&mut c.m, AIDataRecord(a[0]), a[1] as i32) as u32)),
            ("CurveSpeedTable_t::Get", nfs4rc::f_8006dc4c, &[2, 4], |c, a| Some(curve_speed_table_get(&mut c.m, AIDataRecord(a[0]), a[1] as i32) as u32)),
            ("CurveSpeedTable_t::Upgrade", nfs4rc::f_8006dc80, &[2, 5], |c, a| {
                curve_speed_table_upgrade(&mut c.m, AIDataRecord(a[0]), a[1] as i32);
                None
            }),
            ("AccTable_t::Setup", nfs4rc::f_8006da0c, &[1], |c, a| {
                acc_table_setup(c, AIDataRecord(a[0]));
                None
            }),
            ("AIDataRecord_t::Setup", nfs4rc::f_8006d88c, &[3], |c, a| {
                setup(c, AIDataRecord(a[0]));
                None
            }),
            ("AIDataRecord_t::Load", nfs4rc::f_8006d8dc, &[2], |c, a| Some(load(c, AIDataRecord(a[0])) as u32)),
            ("AIDataRecord_t::SaveAndPurge", nfs4rc::f_8006d920, &[1], |c, a| Some(save_and_purge(c, AIDataRecord(a[0])) as u32)),
            ("AIDataRecord_t::AddRecordToCollection", nfs4rc::f_8006d6c4, &[1], |c, a| Some(add_record_to_collection(&mut c.m, AIDataRecord(a[0])) as u32)),
            ("AIDataRecord_t::RemoveRecordFromCollection", nfs4rc::f_8006d700, &[3], |c, a| {
                Some(remove_record_from_collection(&mut c.m, AIDataRecord(a[0])) as u32)
            }),
            ("AIDataRecord_t::AIDataRecord_t", nfs4rc::f_8006d624, &[1, 4, 2], |c, a| Some(record_construct(c, AIDataRecord(a[0]), a[2]).0)),
            ("AIDataRecord_t::~AIDataRecord_t", nfs4rc::f_8006d66c, &[2, 4], |c, a| {
                record_destruct(c, AIDataRecord(a[0]), a[1] as i32);
                None
            }),
            ("AccTable_t::AccTable_t", nfs4rc::f_8006d970, &[1, 2, 5, 4], |c, a| Some(acc_table_construct(c, AIDataRecord(a[0]), a[1], a[2] as i32).0)),
            ("BestLine_t::BestLine_t", nfs4rc::f_8006da78, &[3, 4], |c, a| Some(best_line_construct(c, AIDataRecord(a[0]), a[1] as i32).0)),
            ("TrackCurve_t::TrackCurve_t", nfs4rc::f_8006db0c, &[3, 4], |c, a| Some(track_curve_construct(c, AIDataRecord(a[0]), a[1] as i32).0)),
            ("CurveSpeedTable_t::CurveSpeedTable_t", nfs4rc::f_8006dbbc, &[2, 6, 4], |c, a| {
                Some(curve_speed_table_construct(c, AIDataRecord(a[0]), a[1], a[2] as i32).0)
            }),
            ("AccTable_t::~AccTable_t", nfs4rc::f_8006dd94, &[1, 4], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_acc_table_destruct(c);
                None
            }),
            ("AIDataRecord_t::StartUp1", nfs4rc::f_8006d73c, &[], |c, _| {
                start_up1(c);
                None
            }),
            ("AIDataRecord_t::StartUp2", nfs4rc::f_8006d7a8, &[], |c, _| {
                start_up2(c);
                None
            }),
            ("AIDataRecord_t::CleanUp1", nfs4rc::f_8006d814, &[], |c, _| {
                clean_up1(c);
                None
            }),
        ];
        for (k, (name, original, which, port)) in cases.into_iter().enumerate() {
            let args = move |c: &mut Cpu| which.iter().map(|&w| arg(c, w)).collect();
            against_original(name, 2_000, 0xA40 + k as u64, shuffle, original, &args, port);
        }
    }
}
