//! FECARS.CPP: the front end's car manager (`tCarManager`: the cars' descriptions from
//! fecars.car; each player's garage and pink slips' cars, 32 each, what they paid and sell
//! for; which models are available and viewable; the models in a race's line-up) and the list
//! iterators of the car and colour menus.
//!
//! A car "id" past the descriptions' count (`fNumCars`) is a garage or pink slips' slot:
//! `id - fNumCars`. Car ids are shorts (sign-extended from their low half).

use crate::abi::{self, Saved};
use crate::globals::{CAR_MANAGER, CAR_NAME_TEXT, CAR_SELECTED, FECARS_FILE, FECARS_HEAP_NAME, FECARS_PATH_FORMAT, FRONT_END, LIST_ITERATOR_CAR_COLOR_VTABLE, LIST_ITERATOR_CAR_VTABLE, PATHS, TOURNAMENT_MANAGER, TRACK_MANAGER};
use crate::types::{CarInfo, CarManager, OwnedCarInfo};
use ps1rt::{Cpu, Mem};

/// The garage's and the pink slips' lists (`tOwnedCarInfo[2][32]` each) in the manager.
const GARAGE: u32 = 0x8;
const PINK_SLIPS: u32 = 0x108;
/// `fAvailableCars`, `fViewableCars` (`uchar[48]`), `fPinkSlipsAvailableCars`,
/// `fPinkSlipsViewableCars` (`uchar[2][48]`).
const AVAILABLE: u32 = 0x208;
const VIEWABLE: u32 = 0x238;
const PINK_SLIPS_AVAILABLE: u32 = 0x268;
const PINK_SLIPS_VIEWABLE: u32 = 0x2C8;
/// `tSaveCarInfo`: the owned cars, then the available and viewable flags.
const SAVE_AVAILABLE: u32 = 0x80;
const SAVE_VIEWABLE: u32 = 0xB0;
/// `tfrontEnd`'s `carCountry` (`char[2][48]`), `garageCar`, `pinkSlipsCar` (`char[2]`).
const CAR_COUNTRY: u32 = FRONT_END + 0xC0;
const GARAGE_CAR: u32 = FRONT_END + 0x123;
const PINK_SLIPS_CAR: u32 = FRONT_END + 0x125;
/// `tfrontEnd`'s `gameMode`, `raceType`, `track[0]` and `carListType`.
const GAME_MODE: u32 = FRONT_END + 0x3;
const RACE_TYPE: u32 = FRONT_END + 0x4;
const TRACK: u32 = FRONT_END + 0x23;
const CAR_LIST_TYPE: u32 = FRONT_END + 0x12A;
/// A car's class: the police's, the special cars'.
const CLASS_POLICE: u8 = 7;
const CLASS_SPECIAL: u8 = 8;

/// The short in a register's low half.
fn short(r: u32) -> i32 {
    r as i16 as i32
}

/// The library's `blockmove(from, to, bytes)` (0x800e6ec8).
const BLOCKMOVE: fn(&mut Cpu) = nfs4rc::f_800e6ec8;

/// The `slot`th car of `player`'s `list` (`GARAGE` or `PINK_SLIPS`); the original indexes
/// without checking.
fn owned(this: CarManager, list: u32, player: i32, slot: u32) -> OwnedCarInfo {
    OwnedCarInfo(this.0.wrapping_add(list).wrapping_add((player as u32).wrapping_mul(128)).wrapping_add(slot.wrapping_mul(4)))
}

/// The `i`th description.
fn car(m: &mut Mem, this: CarManager, i: u32) -> CarInfo {
    this.cars(m).at(i)
}

/// `R3000` `div`'s remainder as the recompiled code has it (`break` on a zero divisor does
/// nothing there, and the remainder is the dividend).
fn rem(a: i32, b: i32) -> i32 {
    if b == 0 { a } else { a.wrapping_rem(b) }
}

/// `tCarManager::Initialize` (FECARS.CPP, 0x800162d8): no descriptions, empty garages and
/// pink slips, nothing available or viewable.
pub fn initialize(m: &mut Mem, this: CarManager) {
    this.set_num_cars(m, 0);
    this.set_cars(m, CarInfo(0));
    for player in 0..2 {
        for slot in 0..32 {
            owned(this, GARAGE, player, slot).set_car_id(m, -1);
            owned(this, PINK_SLIPS, player, slot).set_car_id(m, -1);
        }
    }
    for i in 0..48 {
        this.set_available_cars(m, i, 0);
        this.set_viewable_cars(m, i, 0);
    }
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(&mut c.m, CarManager(c.r[4]));
}

/// `tCarManager::GetCarFromID` (FECARS.CPP, 0x80016374): the description of model `id` (0 if
/// there is none), its `fAvailable` set from the manager's.
pub fn get_car_from_id(m: &mut Mem, this: CarManager, id: i32) -> u32 {
    let id = id as i16 as i32;
    let mut i = 0;
    while i < this.num_cars(m) {
        let info = car(m, this, i);
        if info.car_id(m) as i32 == id {
            let available = m.r8(this.0.wrapping_add(AVAILABLE).wrapping_add(id as u32));
            info.set_available(m, available);
            return car(m, this, i).0;
        }
        i += 1;
    }
    0
}

pub fn abi_get_car_from_id(c: &mut Cpu) {
    c.r[2] = get_car_from_id(&mut c.m, CarManager(c.r[4]), c.r[5] as i32);
}

/// `tCarManager::GetCarFromSimID` (FECARS.CPP, 0x800163e4): the description whose simulation
/// number is `sim` (0 if none).
pub fn get_car_from_sim_id(m: &mut Mem, this: CarManager, sim: i32) -> u32 {
    let sim = sim as i16 as i32;
    let mut info = this.cars(m);
    for _ in 0..this.num_cars(m) {
        if info.sim_number(m) as i32 == sim {
            return info.0;
        }
        info = info.at(1);
    }
    0
}

pub fn abi_get_car_from_sim_id(c: &mut Cpu) {
    c.r[2] = get_car_from_sim_id(&mut c.m, CarManager(c.r[4]), c.r[5] as i32);
}

/// `tCarManager::CheapestCarStockPrice` (FECARS.CPP, 0x80016434): the lowest price a model
/// sells for (10,000,000 if none has one).
pub fn cheapest_car_stock_price(m: &mut Mem, this: CarManager) -> i32 {
    let mut best = 10_000_000;
    let mut info = this.cars(m);
    for _ in 0..this.num_cars(m) {
        let price = info.prices(m, 0);
        if price > 0 && price < best {
            best = price;
        }
        info = info.at(1);
    }
    best
}

pub fn abi_cheapest_car_stock_price(c: &mut Cpu) {
    c.r[2] = cheapest_car_stock_price(&mut c.m, CarManager(c.r[4])) as u32;
}

/// `tCarManager::CalcUsedPrice` (FECARS.CPP, 0x8001648c): what the first player's garage car
/// `id` sells for: half its price and its upgrades' (an exotic car three quarters); 0 for a
/// model or an empty slot.
pub fn calc_used_price(m: &mut Mem, this: CarManager, id: i32) -> i32 {
    let id = short(id as u32);
    let n = this.num_cars(m);
    if (id as u32) < n {
        return 0;
    }
    let model = owned(this, GARAGE, 0, (id as u32).wrapping_sub(n)).car_id(m);
    if model == -1 {
        return 0;
    }
    let info = get_car_from_id(m, this, model as i32);
    if info == 0 {
        return 0;
    }
    let info = CarInfo(info);
    let upgrades = owned(this, GARAGE, 0, (id as u32).wrapping_sub(this.num_cars(m))).upgrades(m);
    let mut price = info.prices(m, 0);
    for k in 0..3 {
        if (upgrades >> k) & 1 != 0 {
            price = price.wrapping_add(info.prices(m, k + 1));
        }
    }
    if info.exotic_car(m) != 0 { price.wrapping_mul(3) / 4 } else { price >> 1 }
}

pub fn abi_calc_used_price(c: &mut Cpu) {
    c.r[2] = calc_used_price(&mut c.m, CarManager(c.r[4]), c.r[5] as i32) as u32;
}

/// `tCarManager::PurchaseCar` (FECARS.CPP, 0x80016598): model `id` in `color` into the first
/// free slot of `player`'s garage (the car manager's model), which becomes the player's
/// garage car; its price (0 when the garage is full).
pub fn purchase_car(m: &mut Mem, this: CarManager, id: i32, color: i32, player: i32) -> i32 {
    let info = get_car_from_id(m, CAR_MANAGER, id);
    let player = short(player as u32);
    for slot in 0..32 {
        let e = owned(this, GARAGE, player, slot);
        if e.car_id(m) < 0 {
            e.set_car_id(m, id as i8);
            e.set_car_color(m, color as u8);
            e.set_upgrades(m, 0);
            let n = m.r8(this.0);
            m.w8(GARAGE_CAR.wrapping_add(player as u32), (slot as u8).wrapping_add(n));
            return CarInfo(info).prices(m, 0);
        }
    }
    0
}

pub fn abi_purchase_car(c: &mut Cpu) {
    c.r[2] = purchase_car(&mut c.m, CarManager(c.r[4]), c.r[5] as i32, c.r[6] as i32, c.r[7] as i32) as u32;
}

/// Take slot `slot` out of `player`'s `list`: the cars after it move up and the last slot is
/// emptied (`SellCar`'s and `RemoveFromPinkSlipsList`'s). The slot after it is counted from the
/// id's register and the low half of the count, read after the slot is emptied (a model's id
/// gives a slot before the list, which can be the count).
fn remove_slot(m: &mut Mem, this: CarManager, list: u32, player: i32, slot: u32, id: u32) {
    owned(this, list, player, slot).set_car_id(m, -1);
    let mut from = id.wrapping_sub(m.r16(this.0) as u32).wrapping_add(1);
    while short(from) < 32 {
        let k = short(from) as u32;
        let src = owned(this, list, player, k);
        if src.car_id(m) < 0 {
            break;
        }
        let dst = owned(this, list, player, k.wrapping_sub(1));
        let v = src.car_id(m);
        dst.set_car_id(m, v);
        let v = src.upgrades(m);
        dst.set_upgrades(m, v);
        let v = src.car_color(m);
        dst.set_car_color(m, v);
        from = from.wrapping_add(1);
    }
    owned(this, list, player, (short(from) as u32).wrapping_sub(1)).set_car_id(m, -1);
    owned(this, list, player, 31).set_car_id(m, -1);
}

/// `tCarManager::SellCar` (FECARS.CPP, 0x80016674): `player`'s garage car `id` sold (its used
/// price as the first player's, returned); the cars after it move up and the player's garage
/// car stays on its car, or the one before when it was the last.
pub fn sell_car(c: &mut Cpu, this: CarManager, id: u32, player: u32) -> i32 {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = short(id) as u32;
        r.0[1] = id;
        r.0[2] = player;
        r.0[3] = this.0;
        let price = r.enter(c, 0x8001_648c, 0x8001_66a8, |c| calc_used_price(&mut c.m, this, short(id)));
        let m = &mut c.m;
        let p = short(player);
        let slot = (short(id) as u32).wrapping_sub(this.num_cars(m));
        remove_slot(m, this, GARAGE, p, slot, id);
        let at = GARAGE_CAR.wrapping_add(p as u32);
        let garage = m.r8(at) as u32;
        let n = this.num_cars(m);
        if owned(this, GARAGE, p, garage.wrapping_sub(n)).car_id(m) < 0 {
            m.w8(at, if n < garage { garage as u8 - 1 } else { 0 });
        }
        price
    })
}

pub fn abi_sell_car(c: &mut Cpu) {
    c.r[2] = sell_car(c, CarManager(c.r[4]), c.r[5], c.r[6]) as u32;
}

/// `tCarManager::PurchaseUpgrade` (FECARS.CPP, 0x800167f4): the upgrades in `upgrades` (bits
/// 0..2) that `player`'s garage car `id` doesn't have yet put on it; what they cost.
pub fn purchase_upgrade(m: &mut Mem, this: CarManager, id: i32, upgrades: i32, player: i32) -> i32 {
    let (id, player) = (short(id as u32), short(player as u32));
    let slot = |m: &mut Mem| owned(this, GARAGE, player, (id as u32).wrapping_sub(this.num_cars(m)));
    let model = slot(m).car_id(m);
    let info = CarInfo(get_car_from_id(m, this, model as i32));
    let mut cost = 0i32;
    for k in 0..3 {
        let bit = 1 << k;
        if upgrades & bit & 0xFFFF != 0 {
            let e = slot(m);
            let has = e.upgrades(m);
            if has as i32 & bit == 0 {
                e.set_upgrades(m, bit as u8 | has);
                cost = cost.wrapping_add(info.prices(m, k + 1));
            }
        }
    }
    cost
}

pub fn abi_purchase_upgrade(c: &mut Cpu) {
    c.r[2] = purchase_upgrade(&mut c.m, CarManager(c.r[4]), c.r[5] as i32, c.r[6] as i32, c.r[7] as i32) as u32;
}

/// `tCarManager::RemoveFromPinkSlipsList` (FECARS.CPP, 0x80016900): `player`'s pink slips' car
/// `id` taken out (the cars after it move up); the player's pink slips' car reset as
/// `SellCar` does, but by the garage car's slot (as the original has it).
pub fn remove_from_pink_slips_list(m: &mut Mem, this: CarManager, id: u32, player: u32) {
    let p = short(player);
    let slot = (short(id) as u32).wrapping_sub(this.num_cars(m));
    remove_slot(m, this, PINK_SLIPS, p, slot, id);
    let garage = m.r8(GARAGE_CAR.wrapping_add(p as u32)) as u32;
    let n = this.num_cars(m);
    if owned(this, PINK_SLIPS, p, garage.wrapping_sub(n)).car_id(m) < 0 {
        let at = PINK_SLIPS_CAR.wrapping_add(p as u32);
        let pink = m.r8(at) as u32;
        m.w8(at, if n < pink { (garage as u8).wrapping_sub(1) } else { 0 });
    }
}

pub fn abi_remove_from_pink_slips_list(c: &mut Cpu) {
    remove_from_pink_slips_list(&mut c.m, CarManager(c.r[4]), c.r[5], c.r[6]);
}

/// `tCarManager::AddToPinkSlipsList` (FECARS.CPP, 0x80016a48): model `id` in `color` into
/// the first free slot of `player`'s pink slips' cars, which becomes the player's pink slips'
/// car.
pub fn add_to_pink_slips_list(m: &mut Mem, this: CarManager, id: i32, color: i32, player: i32) {
    let player = short(player as u32);
    for slot in 0..32 {
        let e = owned(this, PINK_SLIPS, player, slot);
        if e.car_id(m) < 0 {
            e.set_car_id(m, id as i8);
            e.set_car_color(m, color as u8);
            e.set_upgrades(m, 0);
            let n = m.r8(this.0);
            m.w8(PINK_SLIPS_CAR.wrapping_add(player as u32), (slot as u8).wrapping_add(n));
            return;
        }
    }
}

pub fn abi_add_to_pink_slips_list(c: &mut Cpu) {
    add_to_pink_slips_list(&mut c.m, CarManager(c.r[4]), c.r[5] as i32, c.r[6] as i32, c.r[7] as i32);
}

/// `tCarManager::AddUpgradesToPinkSlipsList` (FECARS.CPP, 0x80016ac4): `upgrades` (bits 0..2)
/// put on `player`'s pink slips' car `id`.
pub fn add_upgrades_to_pink_slips_list(m: &mut Mem, this: CarManager, id: i32, upgrades: i32, player: i32) {
    let (id, player) = (short(id as u32), short(player as u32));
    let slot = |m: &mut Mem| owned(this, PINK_SLIPS, player, (id as u32).wrapping_sub(this.num_cars(m)));
    let model = slot(m).car_id(m);
    // (for its side effect: the description's fAvailable)
    get_car_from_id(m, this, model as i32);
    for k in 0..3 {
        let bit = 1 << k;
        if upgrades & bit & 0xFFFF != 0 {
            let e = slot(m);
            let has = e.upgrades(m);
            if has as i32 & bit == 0 {
                e.set_upgrades(m, bit as u8 | has);
            }
        }
    }
}

pub fn abi_add_upgrades_to_pink_slips_list(c: &mut Cpu) {
    add_upgrades_to_pink_slips_list(&mut c.m, CarManager(c.r[4]), c.r[5] as i32, c.r[6] as i32, c.r[7] as i32);
}

/// `tCarManager::LoadCars` (FECARS.CPP, 0x80016b9c): `player`'s garage from a save (and for
/// the first player which models are available and viewable).
pub fn load_cars(c: &mut Cpu, this: CarManager, save: u32, player: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        let p = short(player);
        r.0[..3].copy_from_slice(&[p as u32, this.0, save]);
        let garage = owned(this, GARAGE, p, 0).0;
        r.call(c, BLOCKMOVE, &[save, garage, 128], 0x8001_6bd8);
        if p == 0 {
            let m = &mut c.m;
            let mut i = 0;
            while i < this.num_cars(m) {
                let v = m.r8(save + SAVE_AVAILABLE + i);
                m.w8(this.0.wrapping_add(AVAILABLE + i), v);
                let v = m.r8(save + SAVE_VIEWABLE + i);
                m.w8(this.0.wrapping_add(VIEWABLE + i), v);
                i += 1;
            }
        }
    })
}

pub fn abi_load_cars(c: &mut Cpu) {
    load_cars(c, CarManager(c.r[4]), c.r[5], c.r[6]);
}

/// `tCarManager::SaveCars` (FECARS.CPP, 0x80016c38): the first player's garage and which
/// models are available and viewable into a save.
pub fn save_cars(c: &mut Cpu, this: CarManager, save: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[..2].copy_from_slice(&[this.0, save]);
        r.call(c, BLOCKMOVE, &[this.0.wrapping_add(GARAGE), save, 128], 0x8001_6c5c);
        let m = &mut c.m;
        let mut i = 0;
        while i < this.num_cars(m) {
            let v = m.r8(this.0.wrapping_add(AVAILABLE + i));
            m.w8(save + SAVE_AVAILABLE + i, v);
            let v = m.r8(this.0.wrapping_add(VIEWABLE + i));
            m.w8(save + SAVE_VIEWABLE + i, v);
            i += 1;
        }
    })
}

pub fn abi_save_cars(c: &mut Cpu) {
    save_cars(c, CarManager(c.r[4]), c.r[5]);
}

/// `tCarManager::LoadPinkSlipsCars` (FECARS.CPP, 0x80016cb0): `player`'s pink slips' cars and
/// their available and viewable models from a save.
pub fn load_pink_slips_cars(c: &mut Cpu, this: CarManager, save: u32, player: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        let p = short(player);
        r.0[..3].copy_from_slice(&[p as u32, this.0, save]);
        let list = owned(this, PINK_SLIPS, p, 0).0;
        r.call(c, BLOCKMOVE, &[save, list, 128], 0x8001_6cec);
        let m = &mut c.m;
        let base = (p as u32).wrapping_mul(48);
        let mut i = 0;
        while i < this.num_cars(m) {
            let at = this.0.wrapping_add(base).wrapping_add(i);
            let v = m.r8(save + SAVE_AVAILABLE + i);
            m.w8(at.wrapping_add(PINK_SLIPS_AVAILABLE), v);
            let v = m.r8(save + SAVE_VIEWABLE + i);
            m.w8(at.wrapping_add(PINK_SLIPS_VIEWABLE), v);
            i += 1;
        }
    })
}

pub fn abi_load_pink_slips_cars(c: &mut Cpu) {
    load_pink_slips_cars(c, CarManager(c.r[4]), c.r[5], c.r[6]);
}

/// `tCarManager::SavePinkSlipsCars` (FECARS.CPP, 0x80016d54): `player`'s pink slips' cars
/// into a save; first (when `remove` isn't -1) their car `remove` taken out, and after, put
/// back into the car manager's list (with its upgrades, as the player's pink slips' car).
pub fn save_pink_slips_cars(c: &mut Cpu, this: CarManager, save: u32, player: u32, remove: u32) {
    abi::frame(c, 256, |c, sp| {
        let mut r = Saved::of(c);
        let info = sp + 16;
        r.0[1] = short(remove) as u32;
        r.0[2] = player;
        r.0[3] = this.0;
        r.0[4] = save;
        r.0[5] = remove;
        r.0[6] = player;
        let p = short(player);
        if short(remove) != -1 {
            r.0[0] = p as u32;
            r.enter(c, 0x8001_71a0, 0x8001_6db4, |c| get_pink_slips_car(c, this, short(remove) as u32, info, p as u32));
            remove_from_pink_slips_list(&mut c.m, this, short(remove) as u32, p as u32);
        }
        r.0[0] = p as u32;
        let list = owned(this, PINK_SLIPS, p, 0).0;
        r.call(c, BLOCKMOVE, &[list, save, 128], 0x8001_6de4);
        let m = &mut c.m;
        let base = (p as u32).wrapping_mul(48);
        let mut i = 0;
        while i < this.num_cars(m) {
            let at = this.0.wrapping_add(base).wrapping_add(i);
            let v = m.r8(at.wrapping_add(PINK_SLIPS_AVAILABLE));
            m.w8(save + SAVE_AVAILABLE + i, v);
            let v = m.r8(at.wrapping_add(PINK_SLIPS_VIEWABLE));
            m.w8(save + SAVE_VIEWABLE + i, v);
            i += 1;
        }
        if short(remove) != -1 {
            r.0[1] = CAR_MANAGER.0;
            let (model, color) = (m.r8(info) as i8 as i32, m.r8(info + 197) as i32);
            add_to_pink_slips_list(m, CAR_MANAGER, model, color, p);
            let (slot, upgrades) = (m.r8(PINK_SLIPS_CAR.wrapping_add(p as u32)) as i32, m.r8(info + 196) as i32);
            r.enter(c, 0x8001_6ac4, 0x8001_6e88, |c| add_upgrades_to_pink_slips_list(&mut c.m, CAR_MANAGER, slot, upgrades, p));
        }
    })
}

pub fn abi_save_pink_slips_cars(c: &mut Cpu) {
    save_pink_slips_cars(c, CarManager(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

/// `tCarManager::SetClassAvailable` (FECARS.CPP, 0x80016eb0): the models of `class`
/// available or not (and viewable when available).
pub fn set_class_available(m: &mut Mem, this: CarManager, class: u32, on: u32) {
    let mut i = 0;
    while i < this.num_cars(m) {
        let info = car(m, this, i);
        let id = info.car_id(m) as i32;
        if id >= 0 && info.car_class(m) as u32 == class {
            m.w8(this.0.wrapping_add(AVAILABLE).wrapping_add(id as u32), on as u8);
            if on != 0 {
                let id = car(m, this, i).car_id(m) as i32;
                m.w8(this.0.wrapping_add(VIEWABLE).wrapping_add(id as u32), 1);
            }
        }
        i += 1;
    }
}

pub fn abi_set_class_available(c: &mut Cpu) {
    set_class_available(&mut c.m, CarManager(c.r[4]), c.r[5], c.r[6]);
}

/// `tCarManager::SetCarAvailable` (FECARS.CPP, 0x80016f34): `model` available or not (and
/// viewable when available).
pub fn set_car_available(m: &mut Mem, this: CarManager, model: u32, on: u32) {
    let at = this.0.wrapping_add(model);
    m.w8(at.wrapping_add(AVAILABLE), on as u8);
    if on != 0 {
        m.w8(at.wrapping_add(VIEWABLE), 1);
    }
}

pub fn abi_set_car_available(c: &mut Cpu) {
    set_car_available(&mut c.m, CarManager(c.r[4]), c.r[5], c.r[6]);
}

/// `tCarManager::SetClassViewable` (FECARS.CPP, 0x80016f50): the models of `class` viewable
/// or not (a description without a model clears the byte before the list, as the original
/// has it).
pub fn set_class_viewable(m: &mut Mem, this: CarManager, class: u32, on: u32) {
    let mut i = 0;
    while i < this.num_cars(m) {
        let info = car(m, this, i);
        let id = info.car_id(m) as i32;
        let at = this.0.wrapping_add(VIEWABLE).wrapping_add(id as u32);
        if id < 0 {
            m.w8(at, 0);
        } else if info.car_class(m) as u32 == class {
            m.w8(at, on as u8);
        }
        i += 1;
    }
}

pub fn abi_set_class_viewable(c: &mut Cpu) {
    set_class_viewable(&mut c.m, CarManager(c.r[4]), c.r[5], c.r[6]);
}

/// `tCarManager::SetCarViewable` (FECARS.CPP, 0x80016fbc): `model` viewable or not (and
/// available when not viewable, as the original has it).
pub fn set_car_viewable(m: &mut Mem, this: CarManager, model: u32, on: u32) {
    let at = this.0.wrapping_add(model);
    m.w8(at.wrapping_add(VIEWABLE), on as u8);
    if on == 0 {
        m.w8(at.wrapping_add(AVAILABLE), 1);
    }
}

pub fn abi_set_car_viewable(c: &mut Cpu) {
    set_car_viewable(&mut c.m, CarManager(c.r[4]), c.r[5], c.r[6]);
}

/// `tCarManager::GetStockCar` (FECARS.CPP, 0x80016fd8): car `id`'s description into `out`: a
/// model's as it is sold (no upgrades, its flags), a garage car's by `GetGarageCar`; its
/// `fCarIndex` the id.
pub fn get_stock_car(c: &mut Cpu, this: CarManager, id: u32, out: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[..3].copy_from_slice(&[out, this.0, id]);
        let i = short(id) as u32;
        if i >= this.num_cars(&mut c.m) {
            r.enter(c, 0x8001_7094, 0x8001_7018, |c| get_garage_car(c, this, i, out, 0));
            CarInfo(out).set_car_index(&mut c.m, id as u8);
            return;
        }
        let from = this.cars(&mut c.m).at(i).0;
        r.call(c, BLOCKMOVE, &[from, out, CarInfo::SIZE], 0x8001_7048);
        let m = &mut c.m;
        let info = CarInfo(out);
        let model = info.car_id(m) as i32 as u32;
        let v = m.r8(this.0.wrapping_add(AVAILABLE).wrapping_add(model));
        info.set_available(m, v);
        let model = info.car_id(m) as i32 as u32;
        let v = m.r8(this.0.wrapping_add(VIEWABLE).wrapping_add(model));
        info.set_upgrades(m, 0);
        info.set_country(m, 0);
        info.set_viewable(m, v);
        info.set_car_index(m, id as u8);
    })
}

pub fn abi_get_stock_car(c: &mut Cpu) {
    get_stock_car(c, CarManager(c.r[4]), c.r[5], c.r[6]);
}

/// The description of `list`'s car `id` into `out`, available and viewable, with the car's
/// upgrades and colour; `fCarIndex` the id (`GetGarageCar`'s and `GetPinkSlipsCar`'s).
/// `r` has the s-registers the original has at its `blockmove`.
#[allow(clippy::too_many_arguments)]
fn get_owned_car(c: &mut Cpu, r: &Saved, ra: u32, this: CarManager, list: u32, id: u32, out: u32, player: i32) {
    let slot = |m: &mut Mem| owned(this, list, player, (short(id) as u32).wrapping_sub(this.num_cars(m)));
    let model = slot(&mut c.m).car_id(&mut c.m);
    let from = get_car_from_id(&mut c.m, this, model as i32);
    r.call(c, BLOCKMOVE, &[from, out, CarInfo::SIZE], ra);
    let m = &mut c.m;
    let info = CarInfo(out);
    info.set_available(m, 1);
    info.set_viewable(m, 1);
    let v = slot(m).upgrades(m);
    info.set_upgrades(m, v);
    let v = slot(m).car_color(m);
    info.set_country(m, 0);
    info.set_car_index(m, id as u8);
    info.set_color(m, v);
}

/// `tCarManager::GetGarageCar` (FECARS.CPP, 0x80017094): `player`'s garage car `id` (the
/// second player's is the first's while the second has none).
pub fn get_garage_car(c: &mut Cpu, this: CarManager, id: u32, out: u32, player: u32) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        let mut player = player;
        if short(player) == 1 && get_num_owned_cars(&mut c.m, this, 1) <= 0 {
            player = 0;
        }
        let p = short(player);
        r.0[..5].copy_from_slice(&[short(id) as u32, (p as u32).wrapping_mul(128), this.0, out, id]);
        get_owned_car(c, &r, 0x8001_7128, this, GARAGE, id, out, p);
    })
}

pub fn abi_get_garage_car(c: &mut Cpu) {
    get_garage_car(c, CarManager(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

/// `tCarManager::GetPinkSlipsCar` (FECARS.CPP, 0x800171a0): `player`'s pink slips' car `id`.
pub fn get_pink_slips_car(c: &mut Cpu, this: CarManager, id: u32, out: u32, player: u32) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        let p = short(player);
        r.0[..5].copy_from_slice(&[short(id) as u32, this.0, out, (p as u32).wrapping_mul(128), id]);
        get_owned_car(c, &r, 0x8001_7204, this, PINK_SLIPS, id, out, p);
    })
}

pub fn abi_get_pink_slips_car(c: &mut Cpu) {
    get_pink_slips_car(c, CarManager(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

/// `tCarManager::LoadDescription` (FECARS.CPP, 0x8001727c): the descriptions from fecars.car
/// (a count, then the `tCarInfo`s) onto the heap; the models it marks available available and
/// viewable; each one's default colour turned into its place in the colour order.
pub fn load_description(c: &mut Cpu, this: CarManager) {
    abi::frame(c, 112, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let name = sp + 16;
        let dir = c.m.r32(PATHS + 0x94);
        r.call(c, nfs4rc::f_800e50a8, &[name, FECARS_PATH_FORMAT, dir, FECARS_FILE], 0x8001_72b0); // sprintf
        r.enter(c, 0x8001_743c, 0x8001_72b8, |c| release_description(c, this));
        let file = r.call(c, nfs4rc::f_800e63d4, &[name, 16], 0x8001_72c4); // loadfileadr
        r.0[1] = file;
        let n = c.m.r32(file);
        this.set_num_cars(&mut c.m, n);
        let cars = r.call(c, nfs4rc::f_800e5f28, &[FECARS_HEAP_NAME, n.wrapping_mul(CarInfo::SIZE), 0], 0x8001_72f4); // reservememadr
        this.set_cars(&mut c.m, CarInfo(cars));
        let size = this.num_cars(&mut c.m).wrapping_mul(CarInfo::SIZE);
        r.call(c, BLOCKMOVE, &[file + 4, cars, size], 0x8001_731c);
        let m = &mut c.m;
        let mut i = 0u32;
        while (short(i) as u32) < this.num_cars(m) {
            let k = short(i) as u32;
            let info = car(m, this, k);
            if info.available(m) != 0 {
                let id = info.car_id(m) as i32 as u32;
                m.w8(this.0.wrapping_add(AVAILABLE).wrapping_add(id), 1);
                let id = car(m, this, k).car_id(m) as i32 as u32;
                m.w8(this.0.wrapping_add(VIEWABLE).wrapping_add(id), 1);
            }
            let colors = |m: &mut Mem| {
                let info = car(m, this, k);
                info.num_light_colors(m) as u8 as i32 + info.num_dark_colors(m) as u8 as i32
            };
            let mut j = 0u32;
            while short(j) < colors(m) {
                let info = car(m, this, k);
                let order = m.r8(info.0.wrapping_add(0xAF).wrapping_add(short(j) as u32)) as i8 as i32;
                if order == info.default_color(m) as i32 {
                    info.set_default_color(m, j as u8);
                    break;
                }
                j = j.wrapping_add(1);
            }
            i = i.wrapping_add(1);
        }
        r.call(c, nfs4rc::f_800e612c, &[file], 0x8001_7428); // purgememadr
    })
}

pub fn abi_load_description(c: &mut Cpu) {
    load_description(c, CarManager(c.r[4]));
}

/// `tCarManager::ReleaseDescription` (FECARS.CPP, 0x8001743c): the descriptions freed.
pub fn release_description(c: &mut Cpu, this: CarManager) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let cars = this.cars(&mut c.m).0;
        if cars != 0 {
            r.call(c, nfs4rc::f_800e612c, &[cars], 0x8001_7464); // purgememadr
        }
        this.set_cars(&mut c.m, CarInfo(0));
        this.set_num_cars(&mut c.m, 0);
    })
}

pub fn abi_release_description(c: &mut Cpu) {
    release_description(c, CarManager(c.r[4]));
}

/// How many of `player`'s `list` slots hold a car (`GetNumOwnedCars`'s, `GetNumPinkSlipsCars`').
fn count_owned(m: &mut Mem, this: CarManager, list: u32, player: u32) -> i32 {
    let p = short(player);
    let n = (0..32).filter(|&slot| owned(this, list, p, slot).car_id(m) >= 0).count();
    n as i16 as i32
}

/// `tCarManager::GetNumOwnedCars` (FECARS.CPP, 0x8001747c): the cars in `player`'s garage.
pub fn get_num_owned_cars(m: &mut Mem, this: CarManager, player: u32) -> i32 {
    count_owned(m, this, GARAGE, player)
}

pub fn abi_get_num_owned_cars(c: &mut Cpu) {
    c.r[2] = get_num_owned_cars(&mut c.m, CarManager(c.r[4]), c.r[5]) as u32;
}

/// `tCarManager::GetNumTourneyCars` (FECARS.CPP, 0x800174c0): the cars in `player`'s garage
/// the tournament takes (`tTournamentManager::ValidCar`, given the car's model, upgrades and
/// class).
pub fn get_num_tourney_cars(c: &mut Cpu, this: CarManager, player: u32) -> i32 {
    abi::frame(c, 248, |c, sp| {
        let mut r = Saved::of(c);
        let info = CarInfo(sp + 16);
        let base = (short(player) as u32).wrapping_mul(128);
        let mut count = 0u32;
        r.0[2] = 0;
        r.0[3] = this.0;
        for slot in 0..32 {
            r.0[0] = base.wrapping_add(4 * slot);
            r.0[1] = slot;
            let m = &mut c.m;
            let e = OwnedCarInfo(this.0.wrapping_add(GARAGE).wrapping_add(r.0[0]));
            let model = e.car_id(m);
            if model < 0 {
                continue;
            }
            info.set_car_id(m, model);
            let v = e.upgrades(m);
            info.set_upgrades(m, v);
            let desc = CarInfo(get_car_from_id(m, this, model as i32));
            let v = desc.car_class(m);
            info.set_car_class(m, v);
            if r.call(c, nfs4rc::f_80033d60, &[TOURNAMENT_MANAGER, info.0], 0x8001_7528) != 0 {
                count += 1;
                r.0[2] = count;
            }
        }
        count as i16 as i32
    })
}

pub fn abi_get_num_tourney_cars(c: &mut Cpu) {
    c.r[2] = get_num_tourney_cars(c, CarManager(c.r[4]), c.r[5]) as u32;
}

/// `tCarManager::GetNumPinkSlipsCars` (FECARS.CPP, 0x80017568): the cars in `player`'s pink
/// slips' list.
pub fn get_num_pink_slips_cars(m: &mut Mem, this: CarManager, player: u32) -> i32 {
    count_owned(m, this, PINK_SLIPS, player)
}

pub fn abi_get_num_pink_slips_cars(c: &mut Cpu) {
    c.r[2] = get_num_pink_slips_cars(&mut c.m, CarManager(c.r[4]), c.r[5]) as u32;
}

/// `tCarManager::GetClassList` (FECARS.CPP, 0x800175ac): the models of `class` into `list`
/// (up to `max`, the rest of the `max` filled by repeating them); how many there are.
pub fn get_class_list(m: &mut Mem, this: CarManager, class: u32, max: u32, list: u32) -> i32 {
    let max = short(max);
    let mut count = 0u32;
    if this.num_cars(m) != 0 {
        let mut i = 0u32;
        loop {
            let info = car(m, this, i & 0xFFFF);
            if info.car_class(m) as u32 == class {
                let k = short(count);
                if k < max {
                    let id = info.car_id(m) as i32 as u32;
                    m.w32(list.wrapping_add((k as u32).wrapping_mul(4)), id);
                }
                count = count.wrapping_add(1);
            }
            i = i.wrapping_add(1);
            if i & 0xFFFF >= this.num_cars(m) {
                break;
            }
        }
    }
    if ((count & 0xFFFF) as i32) < max {
        let mut i = count;
        loop {
            let k = (i & 0xFFFF) as i32;
            let from = rem(k, short(count));
            i = i.wrapping_add(1);
            let v = m.r32(list.wrapping_add((from as u32).wrapping_mul(4)));
            m.w32(list.wrapping_add((k as u32).wrapping_mul(4)), v);
            if ((i & 0xFFFF) as i32) >= max {
                break;
            }
        }
    }
    short(count)
}

pub fn abi_get_class_list(c: &mut Cpu) {
    c.r[2] = get_class_list(&mut c.m, CarManager(c.r[4]), c.r[5], c.r[6], c.r[7]) as u32;
}

/// `tCarManager::InitializeIngameCarList` (FECARS.CPP, 0x800176c0): no models in the line-up.
pub fn initialize_ingame_car_list(m: &mut Mem) {
    for i in 0..100 {
        m.w8(CAR_SELECTED + i, 0);
    }
}

pub fn abi_initialize_ingame_car_list(c: &mut Cpu) {
    initialize_ingame_car_list(&mut c.m);
}

/// The line-up's byte for `model` (a `tCarModels`) and `order` (a place in the colour order:
/// the bank of 8 colours `order / 8`).
fn selected(m: &mut Mem, model: u32, bank: i32) -> u32 {
    CAR_SELECTED.wrapping_add(m.r32(model)).wrapping_add((bank as u32).wrapping_mul(50))
}

/// `model`'s description and its colour order's entry for `*color`.
fn color_order(m: &mut Mem, this: CarManager, model: u32, color: u32) -> (u32, u32) {
    let id = m.r16(model) as i16 as i32;
    let info = get_car_from_id(m, this, id);
    let k = m.r8(color) as u32;
    (info, info.wrapping_add(0xAF).wrapping_add(k))
}

/// `tCarManager::IsCarAnAddedModel` (FECARS.CPP, 0x80017740): whether `*model` is in the
/// line-up in a colour of the bank of `*color` (any of the bank's).
pub fn is_car_an_added_model(m: &mut Mem, this: CarManager, model: u32, color: u32) -> bool {
    let (_, at) = color_order(m, this, model, color);
    let order = m.r8(at) as i8 as i32;
    let at = selected(m, model, order / 8);
    m.r8(at) != 0
}

pub fn abi_is_car_an_added_model(c: &mut Cpu) {
    c.r[2] = is_car_an_added_model(&mut c.m, CarManager(c.r[4]), c.r[5], c.r[6]) as u32;
}

/// `tCarManager::AddCarToIngameList` (FECARS.CPP, 0x800177c0): `*model` in `*color` into the
/// line-up.
pub fn add_car_to_ingame_list(m: &mut Mem, this: CarManager, model: u32, color: u32) {
    let (_, at) = color_order(m, this, model, color);
    let order = m.r8(at) as i8 as i32;
    let at = selected(m, model, order / 8);
    let v = m.r8(at) | 1u8 << (order & 7);
    m.w8(at, v);
}

pub fn abi_add_car_to_ingame_list(c: &mut Cpu) {
    add_car_to_ingame_list(&mut c.m, CarManager(c.r[4]), c.r[5], c.r[6]);
}

/// `tCarManager::FindSimilarCar` (FECARS.CPP, 0x80017860): a colour of `*model` not yet in the
/// line-up, from `*color` on in the colour order, in its bank of 8 colours first, then the
/// other's: put in the line-up and made `*color`. Returns 1 (found or not).
pub fn find_similar_car(m: &mut Mem, this: CarManager, model: u32, color: u32) -> u32 {
    let (info, at) = color_order(m, this, model, color);
    let order = m.r8(at) as i8 as i32;
    // (a logical shift, as the original's: a negative entry gives no bank)
    let mut bank = (if order < 0 { order + 7 } else { order }) as u32 >> 3;
    let info = CarInfo(info);
    let colors = info.num_light_colors(m) as u8 as i32 + info.num_dark_colors(m) as u8 as i32;
    let colors16 = colors as i16 as i32;
    for _ in 0..2 {
        if colors << 16 != 0 {
            let want = bank as i16 as i32;
            let mut k = 0i32;
            loop {
                let place = rem(m.r8(color) as i32 + k, colors16);
                let entry = m.r8(info.0.wrapping_add(0xAF).wrapping_add(place as u32)) as u32;
                if (entry >> 3) as i32 == want {
                    let at = CAR_SELECTED.wrapping_add(m.r32(model)).wrapping_add((entry >> 3) * 50);
                    let bits = m.r8(at) as i32;
                    let bit = entry & 7;
                    if (bits >> bit) & 1 == 0 {
                        m.w8(at, (bits | 1 << bit) as u8);
                        let place = rem(m.r8(color) as i32 + k, colors16);
                        m.w8(color, place as u8);
                        return 1;
                    }
                }
                k = (k + 1) as i16 as i32;
                if k >= colors {
                    break;
                }
            }
        }
        bank = 1u32.wrapping_sub(bank);
    }
    1
}

pub fn abi_find_similar_car(c: &mut Cpu) {
    c.r[2] = find_similar_car(&mut c.m, CarManager(c.r[4]), c.r[5], c.r[6]);
}

/// The base iterator's constructor and destructor (`tListIterator`, FEMENU.CPP), still
/// recompiled.
const LIST_ITERATOR: fn(&mut Cpu) = nfs4rc::f_80023df0;
const LIST_ITERATOR_DTOR: fn(&mut Cpu) = nfs4rc::f_80023e14;

/// `tListIteratorCar::tListIteratorCar` (FECARS.CPP, 0x80017a14): the car menu's iterator
/// over `value` (a car per player) of `manager`'s cars (filter 1: the models).
pub fn list_iterator_car(c: &mut Cpu, this: u32, value: u32, manager: u32) -> u32 {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[..2].copy_from_slice(&[this, manager]);
        r.call(c, LIST_ITERATOR, &[this, 0, value], 0x8001_7a3c);
        let m = &mut c.m;
        m.w32(this + 12, LIST_ITERATOR_CAR_VTABLE);
        m.w32(this + 20, manager);
        m.w32(this + 16, 1);
        m.w32(this + 24, 0);
        this
    })
}

pub fn abi_list_iterator_car(c: &mut Cpu) {
    c.r[2] = list_iterator_car(c, c.r[4], c.r[5], c.r[6]);
}

/// The destructor of an iterator whose vtable is `vtable` (`tListIteratorCar`'s and
/// `tListIteratorCarColor`'s): the base's, given the caller's flags in a1.
fn destroy_iterator(c: &mut Cpu, this: u32, vtable: u32, ra: u32) {
    abi::frame(c, 24, |c, _| {
        c.m.w32(this + 12, vtable);
        let flags = c.r[5];
        Saved::of(c).call(c, LIST_ITERATOR_DTOR, &[this, flags], ra);
    })
}

/// `tListIteratorCar::~tListIteratorCar` (FECARS.CPP, 0x80017a70).
pub fn abi_list_iterator_car_dtor(c: &mut Cpu) {
    destroy_iterator(c, c.r[4], LIST_ITERATOR_CAR_VTABLE, 0x8001_7a88);
}

/// A menu's player: -1 (all of them) is the first.
fn menu_player(player: u32) -> i32 {
    short(if player == u32::MAX { 0 } else { player })
}

/// `tListIteratorCar::Value` (FECARS.CPP, 0x80017a98): `player`'s car.
pub fn list_iterator_car_value(m: &mut Mem, this: u32, player: u32) -> u32 {
    let value = m.r32(this + 4);
    m.r8(value.wrapping_add(menu_player(player) as u32)) as u32
}

pub fn abi_list_iterator_car_value(c: &mut Cpu) {
    c.r[2] = list_iterator_car_value(&mut c.m, c.r[4], c.r[5]);
}

/// `tListIteratorCar::TextValue` (FECARS.CPP, 0x80017ac4): the text id of `player`'s car's
/// name (by the name length `fNameLength`).
pub fn list_iterator_car_text_value(c: &mut Cpu, this: u32, player: u32) -> u32 {
    abi::frame(c, 8, |c, sp| {
        let m = &mut c.m;
        // the first text ids, copied onto the stack
        let w = m.r32(CAR_NAME_TEXT);
        m.w32(sp, w);
        let h = m.r16(CAR_NAME_TEXT + 4);
        m.w16(sp + 4, h);
        let p = menu_player(player);
        let manager = CarManager(m.r32(this + 20));
        let values = m.r32(this + 4);
        let value = m.r8(values.wrapping_add(p as u32)) as u32;
        let n = manager.num_cars(m);
        let length = m.r32(this + 24);
        let base = m.r16(sp.wrapping_add(length.wrapping_mul(2))) as i16 as i32;
        let id = if value < n {
            car(m, manager, value).car_id(m)
        } else {
            let list = if m.r32(this + 16) & 0x20 != 0 { PINK_SLIPS } else { GARAGE };
            owned(manager, list, p, value - n).car_id(m)
        };
        base.wrapping_add(id as i32) as u32
    })
}

pub fn abi_list_iterator_car_text_value(c: &mut Cpu) {
    c.r[2] = list_iterator_car_text_value(c, c.r[4], c.r[5]);
}

/// `tListIteratorCar::AdjustPosition` (FECARS.CPP, 0x80017be4): `player`'s car moved by
/// `step` through the list the filter picks (the models; the garage, or the first player's
/// while the second has none; the pink slips' cars), kept in it; the police's models step
/// through their countries' liveries (0..4) first. Moves on until `ValidCar` takes the car,
/// or the car and its livery are as they were.
pub fn adjust_position(c: &mut Cpu, this: u32, player: u32, step: u32) {
    abi::frame(c, 72, |c, sp| {
        let mut r = Saved::of(c);
        // (a1 homed in the caller's frame)
        c.m.w32(sp + 76, player);
        let p = if player == u32::MAX { 0 } else { player & 0xFFFF };
        r.0[2] = this;
        r.0[3] = p;
        r.0[7] = step;
        r.0[0] = 0;
        let m = &mut c.m;
        let mgr = |m: &mut Mem| CarManager(m.r32(this + 20));
        let num = |m: &mut Mem| {
            let g = mgr(m);
            g.num_cars(m)
        };
        let filter = m.r32(this + 16);
        let (first, end);
        if filter & 0x89 != 0 {
            end = num(m);
            first = 0;
        } else {
            let pl = short(p);
            let g = mgr(m);
            let list = if filter & 0x42 != 0 {
                let mut at = owned(g, GARAGE, pl, 0).0;
                if pl == 1 && get_num_owned_cars(m, g, 1) <= 0 {
                    at = mgr(m).0 + GARAGE;
                }
                at
            } else {
                owned(g, PINK_SLIPS, pl, 0).0
            };
            first = num(m);
            let mut count = 0u32;
            if (m.r8(list) as i8) >= 0 {
                let mut at = list;
                loop {
                    at = at.wrapping_add(4);
                    count += 1;
                    if (m.r8(at) as i8) < 0 {
                        break;
                    }
                }
            }
            end = count.wrapping_add(first);
        }
        r.0[4] = end;
        r.0[5] = first;
        let (end_i, first_i) = (end as i32, first as i32);
        let pl = short(p);
        // the player's position in the list, and the byte there
        let pos = |m: &mut Mem| m.r32(this + 4).wrapping_add(pl as u32);
        let at_pos = |m: &mut Mem| {
            let a = pos(m);
            m.r8(a)
        };
        let set_pos = |m: &mut Mem, v: u8| {
            let a = pos(m);
            m.w8(a, v)
        };
        // the description of model `v`, and the player's livery byte for a description's model
        let desc = |m: &mut Mem, v: u32| {
            let g = mgr(m);
            car(m, g, v).0
        };
        let livery = |m: &mut Mem, info: u32| CAR_COUNTRY.wrapping_add((pl as u32).wrapping_mul(48)).wrapping_add(m.r8(info) as i8 as i32 as u32);
        let set_livery = |m: &mut Mem, info: u32, v: u8| {
            let a = livery(m, info);
            m.w8(a, v)
        };
        if !((at_pos(m) as i32) < end_i) {
            set_pos(m, end.wrapping_sub(1) as u8);
        }
        if (at_pos(m) as i32) < first_i {
            set_pos(m, first as u8);
        }
        if first_i >= end_i {
            return;
        }
        // the car and its livery as they were
        let was = at_pos(m);
        m.w8(sp + 16, was);
        let mut car_at = 0u32;
        if (was as u32) < num(m) {
            car_at = desc(m, was as u32);
            let a = livery(m, car_at);
            let v = m.r8(a);
            m.w8(sp + 24, v);
        } else {
            m.w8(sp + 24, 0);
        }
        r.0[0] = car_at;
        r.0[1] = pl as u32;
        r.0[3] = (pl as u32).wrapping_mul(48);
        r.0[6] = FRONT_END;
        r.0[8] = 4;
        let step8 = step as u8;
        let up = short(step) > 0;
        loop {
            let m = &mut c.m;
            let v = at_pos(m) as u32;
            if v < num(m) {
                car_at = desc(m, v);
                if m.r8(car_at + 2) == CLASS_POLICE {
                    // through the liveries first
                    let cc = livery(m, car_at);
                    let l = m.r8(cc).wrapping_add(step8);
                    m.w8(cc, l);
                    let l = m.r8(cc) as i8;
                    if l >= 5 {
                        m.w8(cc, 0);
                        let v = at_pos(m).wrapping_add(1);
                        set_pos(m, v);
                        if (v as i32) < end_i {
                            car_at = desc(m, v as u32);
                            set_livery(m, car_at, 0);
                        }
                    } else if l < 0 {
                        m.w8(cc, 4);
                        let v = at_pos(m).wrapping_sub(1);
                        set_pos(m, v);
                        if (v as i32) >= first_i {
                            car_at = desc(m, v as u32);
                            set_livery(m, car_at, 4);
                        }
                    }
                } else {
                    set_livery(m, car_at, if up { 0 } else { 4 });
                    let v = at_pos(m).wrapping_add(step8);
                    set_pos(m, v);
                    let now = at_pos(m);
                    if up {
                        if (now as i8 as i32) < end_i {
                            car_at = desc(m, now as u32);
                            set_livery(m, car_at, 0);
                        }
                    } else if short(step) < 0 && (now as i8 as i32) >= first_i {
                        car_at = desc(m, now as u32);
                        set_livery(m, car_at, 4);
                    }
                }
            } else {
                set_pos(m, step8.wrapping_add(v as u8));
            }
            // kept in the list (wrapping round)
            if !((at_pos(m) as i8 as i32) < end_i) {
                set_pos(m, first as u8);
                let v = at_pos(m) as u32;
                if v < num(m) {
                    car_at = desc(m, v);
                    set_livery(m, car_at, 0);
                }
            }
            if (at_pos(m) as i8 as i32) < first_i {
                set_pos(m, end.wrapping_sub(1) as u8);
                let v = at_pos(m) as u32;
                if v < num(m) {
                    car_at = desc(m, v);
                    set_livery(m, car_at, 4);
                }
            }
            r.0[0] = car_at;
            let now = at_pos(m);
            if now == m.r8(sp + 16) {
                if !((now as i8 as i32) < num(m) as i32) {
                    return;
                }
                // (the livery read signed, the one kept unsigned)
                let a = livery(m, car_at);
                if m.r8(a) as i8 as i32 == m.r8(sp + 24) as i32 {
                    return;
                }
            }
            let homed = c.m.r32(sp + 76);
            let valid = r.enter(c, 0x8001_8214, 0x8001_8198, |c| valid_car(c, this, homed, now as u32));
            if valid ^ 1 == 0 {
                return;
            }
        }
    })
}

pub fn abi_adjust_position(c: &mut Cpu) {
    adjust_position(c, c.r[4], c.r[5], c.r[6]);
}

/// `tListIteratorCar::Increment` (FECARS.CPP, 0x800181d4): `player`'s next car.
pub fn list_iterator_car_increment(c: &mut Cpu, this: u32, player: u32) {
    abi::frame(c, 24, |c, _| {
        Saved::of(c).enter(c, 0x8001_7be4, 0x8001_81e4, |c| adjust_position(c, this, player, 1));
    })
}

pub fn abi_list_iterator_car_increment(c: &mut Cpu) {
    list_iterator_car_increment(c, c.r[4], c.r[5]);
}

/// `tListIteratorCar::Decrement` (FECARS.CPP, 0x800181f4): `player`'s previous car.
pub fn list_iterator_car_decrement(c: &mut Cpu, this: u32, player: u32) {
    abi::frame(c, 24, |c, _| {
        Saved::of(c).enter(c, 0x8001_7be4, 0x8001_8204, |c| adjust_position(c, this, player, u32::MAX));
    })
}

pub fn abi_list_iterator_car_decrement(c: &mut Cpu) {
    list_iterator_car_decrement(c, c.r[4], c.r[5]);
}

/// `tListIteratorCar::ValidCar` (FECARS.CPP, 0x80018214): whether `player` may pick `car` (a
/// position in the list) under the iterator's filter: a garage or pink slips' car that is
/// there (in a pursuit one that may pursue; for the tournament filter one the tournament
/// takes); a model that is viewable (in a pursuit one that may pursue) of a class the filter
/// takes (the police's in a country the car has, for the track's country or with the cheat;
/// not the police Diablo in some modes), in a tournament one it takes.
pub fn valid_car(c: &mut Cpu, this: u32, player: u32, car_pos: u32) -> u32 {
    abi::frame(c, 296, |c, sp| {
        let mut r = Saved::of(c);
        let info_local = sp + 16;
        let track_info = sp + 224;
        let mut s1 = if player == u32::MAX { 0 } else { player };
        let mut s3 = s1;
        let mut s2 = car_pos;
        let mut s0 = 0u32;
        let mgr = |m: &mut Mem| CarManager(m.r32(this + 20));
        let manager = mgr(&mut c.m);
        if short(s1) == 1 && get_num_owned_cars(&mut c.m, manager, 1) <= 0 {
            s3 = 0;
        }
        r.0[4] = this;
        let m = &mut c.m;
        let manager = mgr(m);
        let filter = |m: &mut Mem| m.r32(this + 16);
        let pos = s2 & 0xFF;
        if pos >= manager.num_cars(m) {
            s2 = s2.wrapping_sub(m.r8(manager.0) as u32);
            let slot = s2 & 0xFF;
            let f = filter(m);
            if f & 0x20 != 0 {
                return (owned(manager, PINK_SLIPS, short(s1), slot).car_id(m) >= 0) as u32;
            }
            if f & 0x42 == 0 {
                return 0;
            }
            let model = owned(manager, GARAGE, short(s3), slot).car_id(m);
            if model >= 0 {
                s0 = 1;
            }
            let info = CarInfo(get_car_from_id(m, manager, model as i32));
            if m.r8(RACE_TYPE) == 1 && info.pursuit_available(m) == 0 {
                return 0;
            }
            if s0 == 0 {
                return 0;
            }
            if filter(m) & 0x40 == 0 {
                return s0;
            }
            r.0[..4].copy_from_slice(&[s0, s1, s2, s3]);
            let manager = mgr(m);
            let id = short((s2 & 0xFF).wrapping_add(m.r16(manager.0) as u32)) as u32;
            r.enter(c, 0x8001_7094, 0x8001_837c, |c| get_garage_car(c, manager, id, info_local, 0));
            return r.call(c, nfs4rc::f_80033d60, &[TOURNAMENT_MANAGER, info_local], 0x8001_85b8);
        }
        let info = car(m, manager, pos);
        let model = info.car_id(m) as i32;
        if model < 0 || m.r8(manager.0.wrapping_add(VIEWABLE).wrapping_add(model as u32)) == 0 {
            return s0;
        }
        if m.r8(RACE_TYPE) == 1 && info.pursuit_available(m) == 0 {
            return 0;
        }
        let g = mgr(m);
        let info = car(m, g, s2 & 0xFF);
        let class = info.car_class(m);
        let f = filter(m);
        if class == CLASS_POLICE {
            if f & 0xC != 0 {
                s1 = (model as i16 as i32 as u32).wrapping_add((short(s1) as u32).wrapping_mul(48)).wrapping_add(FRONT_END);
                let country = m.r8(s1.wrapping_add(0xC0)) as i8 as i32;
                if (info.countries(m) as i32 >> (country & 31)) & 1 != 0 {
                    r.0[..4].copy_from_slice(&[s0, s1, s2, s3]);
                    let track = m.r8(TRACK) as u32;
                    r.call(c, nfs4rc::f_80021c04, &[TRACK_MANAGER, track, track_info], 0x8001_8514); // tTrackManager::GetTrack
                    // (with the cheat, or for the track's country)
                    if r.call(c, nfs4rc::f_8002373c, &[11], 0x8001_851c) != 0 || c.m.r8(sp + 248) as i32 == c.m.r8(s1.wrapping_add(0xC0)) as i8 as i32 {
                        s0 = 1;
                    }
                }
            }
        } else if class < 8 {
            if f & 0x81 != 0 {
                s0 = 1;
            }
            if model as i16 == 28 && (m.r8(CAR_LIST_TYPE) == 1 || m.r8(GAME_MODE) == 1 || m.r8(RACE_TYPE) != 0) {
                s0 = 0;
            }
        } else if class == CLASS_SPECIAL && f & 0x10 != 0 {
            s0 = 1;
        }
        let m = &mut c.m;
        if s0 == 0 {
            return 0;
        }
        if filter(m) & 1 == 0 || m.r8(RACE_TYPE) != 2 {
            return s0;
        }
        r.0[..4].copy_from_slice(&[s0, s1, s2, s3]);
        let g = mgr(m);
        let info = car(m, g, s2 & 0xFF).0;
        r.call(c, nfs4rc::f_80033d60, &[TOURNAMENT_MANAGER, info], 0x8001_85b8) // tTournamentManager::ValidCar
    })
}

pub fn abi_valid_car(c: &mut Cpu) {
    c.r[2] = valid_car(c, c.r[4], c.r[5], c.r[6]);
}

/// `tListIteratorCarColor::tListIteratorCarColor` (FECARS.CPP, 0x800185e0): the colour menu's
/// iterator over `value` (`index_size` colours per player) for the car `player_car[*player]`.
pub fn list_iterator_car_color(c: &mut Cpu, this: u32, value: u32, player: u32, player_car: u32, index_size: u32, manager: u32) -> u32 {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[..5].copy_from_slice(&[this, index_size, manager, player, player_car]);
        r.call(c, LIST_ITERATOR, &[this, 0, value], 0x8001_8620);
        let m = &mut c.m;
        m.w32(this + 12, LIST_ITERATOR_CAR_COLOR_VTABLE);
        m.w32(this + 16, player);
        m.w32(this + 20, player_car);
        m.w32(this + 24, index_size);
        m.w32(this + 28, manager);
        this
    })
}

pub fn abi_list_iterator_car_color(c: &mut Cpu) {
    let (index_size, manager) = (abi::arg(c, 4), abi::arg(c, 5));
    c.r[2] = list_iterator_car_color(c, c.r[4], c.r[5], c.r[6], c.r[7], index_size, manager);
}

/// `tListIteratorCarColor::~tListIteratorCarColor` (FECARS.CPP, 0x80018660).
pub fn abi_list_iterator_car_color_dtor(c: &mut Cpu) {
    destroy_iterator(c, c.r[4], LIST_ITERATOR_CAR_COLOR_VTABLE, 0x8001_8678);
}

/// The colour iterator's player's row of colours (`*fPlayer * fIndexSize`) and their car's
/// description.
fn color_row(m: &mut Mem, this: u32) -> (u32, CarInfo) {
    let at = m.r32(this + 16);
    let player = m.r8(at) as u32;
    let at = m.r32(this + 20).wrapping_add(player);
    let car_index = m.r8(at) as u32;
    let row = player.wrapping_mul(m.r32(this + 24));
    let manager = CarManager(m.r32(this + 28));
    (row, car(m, manager, car_index))
}

/// The player's colour for their car in `row`.
fn color_at(m: &mut Mem, this: u32, row: u32, info: CarInfo) -> u32 {
    m.r32(this + 4).wrapping_add(row).wrapping_add(info.car_id(m) as i32 as u32)
}

/// The player's colour for their car, and the car's description.
fn color_entry(m: &mut Mem, this: u32) -> (u32, CarInfo) {
    let (row, info) = color_row(m, this);
    (color_at(m, this, row, info), info)
}

/// `tListIteratorCarColor::Value` (FECARS.CPP, 0x80018688): the player's colour for their car.
pub fn list_iterator_car_color_value(m: &mut Mem, this: u32) -> u32 {
    let (at, _) = color_entry(m, this);
    m.r8(at) as u32
}

pub fn abi_list_iterator_car_color_value(c: &mut Cpu) {
    c.r[2] = list_iterator_car_color_value(&mut c.m, c.r[4]);
}

/// `tListIteratorCarColor::TextValue` (FECARS.CPP, 0x800186e8): no text.
pub fn abi_list_iterator_car_color_text_value(c: &mut Cpu) {
    c.r[2] = 0;
}

/// `tListIteratorCarColor::Increment` (FECARS.CPP, 0x800186f0): the next colour (round to the
/// first after the car's last).
pub fn list_iterator_car_color_increment(m: &mut Mem, this: u32) {
    let (row, info) = color_row(m, this);
    let at = color_at(m, this, row, info);
    let v = m.r8(at).wrapping_add(1);
    m.w8(at, v);
    let at = color_at(m, this, row, info);
    let colors = info.num_light_colors(m) as u8 as i32 + info.num_dark_colors(m) as u8 as i32;
    if m.r8(at) as i32 >= colors {
        m.w8(at, 0);
    }
}

pub fn abi_list_iterator_car_color_increment(c: &mut Cpu) {
    list_iterator_car_color_increment(&mut c.m, c.r[4]);
}

/// `tListIteratorCarColor::Decrement` (FECARS.CPP, 0x80018788): the previous colour (round to
/// the car's last).
pub fn list_iterator_car_color_decrement(m: &mut Mem, this: u32) {
    let (at, info) = color_entry(m, this);
    let v = m.r8(at);
    let colors = info.num_light_colors(m) as u8 as i32 + info.num_dark_colors(m) as u8 as i32;
    m.w8(at, if v != 0 { v - 1 } else { (colors - 1) as u8 });
}

pub fn abi_list_iterator_car_color_decrement(c: &mut Cpu) {
    list_iterator_car_color_decrement(&mut c.m, c.r[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

    /// The test's structures, in RAM the front end leaves zero: a car iterator and its values,
    /// a colour iterator, its values, player and cars; a save, a description, a class list, a
    /// model and a colour.
    const IT_CAR: u32 = 0x8014_0000;
    const IT_CAR_VALUES: u32 = 0x8014_0100;
    const IT_COLOR: u32 = 0x8014_0040;
    const IT_COLOR_VALUES: u32 = 0x8014_0200;
    const IT_COLOR_PLAYER: u32 = 0x8014_0300;
    const IT_COLOR_CARS: u32 = 0x8014_0310;
    const IT_NEW: u32 = 0x8014_0080;
    const SAVE: u32 = 0x8014_1000;
    const OUT: u32 = 0x8014_2000;
    const LIST: u32 = 0x8014_3000;
    const MODEL: u32 = 0x8014_4000;
    const COLOR: u32 = 0x8014_4010;
    /// The front end's descriptions (fecars.car's).
    const CARS: u32 = 45;
    /// The stack's top in the front end's tests (main RAM).
    const TOP: u32 = 0x801F_FE00;

    fn pick(rng: &mut Rng, n: u32) -> u32 {
        (rng.next() % n as u64) as u32
    }

    /// A short, now and then with garbage in its high half (the original takes the low half).
    fn short_arg(rng: &mut Rng, v: i32) -> u32 {
        if pick(rng, 8) == 0 { (v as u32 & 0xFFFF) | (rng.next() as u32 & 0xFFFF_0000) } else { v as u32 }
    }

    /// The car select's RAM with the garages and pink slips filled (a few holes), the models'
    /// flags, the front end's mode, race type, cars and liveries, a few descriptions' class,
    /// colours, prices and flags, the line-up's bits; and the test's structures.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        let ids: Vec<i8> = (0..CARS).map(|i| CAR_MANAGER.cars(m).at(i).car_id(m)).collect();
        for list in [GARAGE, PINK_SLIPS] {
            for p in 0..2 {
                let k = pick(rng, 34);
                for s in 0..32 {
                    let e = owned(CAR_MANAGER, list, p, s);
                    if s < k {
                        let id = if pick(rng, 10) == 0 { pick(rng, 60) as i8 } else { ids[pick(rng, CARS) as usize] };
                        e.set_car_id(m, id);
                        e.set_upgrades(m, if pick(rng, 6) == 0 { rng.next() as u8 } else { pick(rng, 8) as u8 });
                        e.set_car_color(m, pick(rng, 16) as u8);
                    } else {
                        e.set_car_id(m, -1);
                    }
                }
                if pick(rng, 4) == 0 {
                    owned(CAR_MANAGER, list, p, pick(rng, 32)).set_car_id(m, -(pick(rng, 3) as i8) - 1);
                }
            }
        }
        for i in 0..48 + 48 + 96 + 96 {
            m.w8(CAR_MANAGER.0 + AVAILABLE + i, (pick(rng, 4) != 0) as u8);
        }
        m.w8(RACE_TYPE, pick(rng, 4) as u8);
        m.w8(GAME_MODE, pick(rng, 3) as u8);
        m.w8(CAR_LIST_TYPE, pick(rng, 3) as u8);
        m.w8(TRACK, pick(rng, 20) as u8);
        for p in 0..2 {
            let low = |rng: &mut Rng| if pick(rng, 8) == 0 { 40 } else { 0 };
            let v = (CARS + pick(rng, 34)) as u8 - low(rng);
            m.w8(GARAGE_CAR + p, v);
            let v = (CARS + pick(rng, 34)) as u8 - low(rng);
            m.w8(PINK_SLIPS_CAR + p, v);
            for i in 0..48 {
                let v = match pick(rng, 10) {
                    0 => rng.next() as u8,
                    1..=4 => pick(rng, 5) as u8,
                    _ => 0,
                };
                m.w8(CAR_COUNTRY + 48 * p + i, v);
            }
        }
        for _ in 0..pick(rng, 5) {
            let info = CAR_MANAGER.cars(m).at(pick(rng, CARS));
            info.set_car_class(m, pick(rng, 10) as u8);
            info.set_exotic_car(m, pick(rng, 2) as u8);
            info.set_pursuit_available(m, pick(rng, 2) as u8);
            info.set_available(m, pick(rng, 2) as u8);
            info.set_default_color(m, pick(rng, 16) as u8);
            info.set_countries(m, rng.next() as u8);
            info.set_num_light_colors(m, pick(rng, 9) as i8);
            info.set_num_dark_colors(m, pick(rng, 9) as i8);
            for k in 0..16 {
                let v = if pick(rng, 8) == 0 { rng.next() as u8 } else { pick(rng, 16) as u8 };
                m.w8(info.0 + 0xAF + k, v);
            }
            for k in 0..4 {
                let v = [0, 1, 20_000, -5, rng.next() as i32][pick(rng, 5) as usize];
                info.set_prices(m, k, v);
            }
        }
        for i in 0..100 {
            let v = if pick(rng, 3) == 0 { rng.next() as u8 } else { 0 };
            m.w8(CAR_SELECTED + i, v);
        }
        // the car iterator: a filter of the menus' bits, the name length
        m.w32(IT_CAR + 4, IT_CAR_VALUES);
        m.w32(IT_CAR + 12, LIST_ITERATOR_CAR_VTABLE);
        let mut filter = 0;
        for bit in [1, 2, 4, 8, 0x10, 0x20, 0x40, 0x80] {
            if pick(rng, 3) == 0 {
                filter |= bit;
            }
        }
        m.w32(IT_CAR + 16, filter);
        m.w32(IT_CAR + 20, CAR_MANAGER.0);
        m.w32(IT_CAR + 24, pick(rng, 3));
        for p in 0..2 {
            m.w8(IT_CAR_VALUES + p, pick(rng, CARS + 34) as u8);
        }
        m.w32(IT_COLOR + 4, IT_COLOR_VALUES);
        m.w32(IT_COLOR + 12, LIST_ITERATOR_CAR_COLOR_VTABLE);
        m.w32(IT_COLOR + 16, IT_COLOR_PLAYER);
        m.w32(IT_COLOR + 20, IT_COLOR_CARS);
        m.w32(IT_COLOR + 24, 48);
        m.w32(IT_COLOR + 28, CAR_MANAGER.0);
        for i in 0..96 {
            m.w8(IT_COLOR_VALUES + i, pick(rng, 16) as u8);
        }
        m.w8(IT_COLOR_PLAYER, pick(rng, 2) as u8);
        for p in 0..2 {
            m.w8(IT_COLOR_CARS + p, pick(rng, CARS) as u8);
        }
        for i in 0..224 {
            let v = if i < 128 && i % 4 == 0 && pick(rng, 3) == 0 { 0xFF } else { pick(rng, 50) as u8 };
            m.w8(SAVE + i, v);
        }
        m.w32(MODEL, ids[pick(rng, CARS) as usize] as u32);
        m.w8(COLOR, pick(rng, 16) as u8);
    }

    /// An id: a model (or none), or a garage slot.
    fn id_arg(rng: &mut Rng) -> u32 {
        let v = if pick(rng, 3) == 0 { pick(rng, 55) as i32 - 2 } else { (CARS + pick(rng, 34)) as i32 - 1 };
        short_arg(rng, v)
    }

    fn player_arg(rng: &mut Rng) -> u32 {
        let p = pick(rng, 2) as i32;
        short_arg(rng, p)
    }

    /// A menu player: -1 (all), 0 or 1.
    fn menu_player_arg(rng: &mut Rng) -> u32 {
        [u32::MAX, 0, 1, 1][pick(rng, 4) as usize]
    }

    /// The arguments of a walk through the car list (`AdjustPosition`: the iterator, a player
    /// and the step; `Increment`, `Decrement` given theirs), the liveries made ones the walk
    /// comes back to as the game has them: the police's 0..4, the other models' what a step
    /// leaves on them (0 going up, 4 going down); and a list's last slot empty (a full list has
    /// no end, and the count runs on through the flags after it). Otherwise, with no car valid,
    /// the original walks for ever.
    fn walk_args(c: &mut Cpu, rng: &mut Rng, step: Option<u32>) -> Vec<u32> {
        let player = menu_player_arg(rng);
        let s = step.unwrap_or([1, u32::MAX][pick(rng, 2) as usize]);
        let m = &mut c.m;
        for list in [GARAGE, PINK_SLIPS] {
            for p in 0..2 {
                owned(CAR_MANAGER, list, p, 31).set_car_id(m, -1);
            }
        }
        for i in 0..CARS {
            let info = CAR_MANAGER.cars(m).at(i);
            let (id, police) = (info.car_id(m) as i32, info.car_class(m) == CLASS_POLICE);
            for p in 0..2 {
                let at = CAR_COUNTRY.wrapping_add(48 * p).wrapping_add(id as u32);
                let v = if police { m.r8(at) % 5 } else if s == 1 { 0 } else { 4 };
                m.w8(at, v);
            }
        }
        match step {
            None => vec![IT_CAR, player, s],
            Some(_) => vec![IT_CAR, player],
        }
    }

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;
    type Args = fn(&mut Cpu, &mut Rng) -> Vec<u32>;
    /// A function's name, its original, its arguments and its port.
    type Case = (&'static str, fn(&mut Cpu), Args, Port);

    fn run_cases(cases: &[Case], seed: u64) {
        let Some(ram) = fe_ram() else { return };
        for (k, &(name, original, args, port)) in cases.iter().enumerate() {
            // (the arguments from the case's own generator: `against_original` hands the CPU)
            let args_rng = std::cell::RefCell::new(Rng(seed ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 600, seed + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), port);
        }
    }

    #[test]
    fn car_manager_fuzz() {
        let cases: &[Case] = &[
            ("Initialize", nfs4rc::f_800162d8, |_, _| vec![CAR_MANAGER.0], |c, _| {
                initialize(&mut c.m, CAR_MANAGER);
                None
            }),
            ("GetCarFromID", nfs4rc::f_80016374, |_, r| {
                let id = pick(r, 60) as i32 - 3;
                vec![CAR_MANAGER.0, short_arg(r, id)]
            }, |c, a| Some(get_car_from_id(&mut c.m, CAR_MANAGER, a[1] as i32))),
            ("GetCarFromSimID", nfs4rc::f_800163e4, |_, r| {
                let id = pick(r, 60) as i32 - 3;
                vec![CAR_MANAGER.0, short_arg(r, id)]
            }, |c, a| Some(get_car_from_sim_id(&mut c.m, CAR_MANAGER, a[1] as i32))),
            ("CheapestCarStockPrice", nfs4rc::f_80016434, |_, _| vec![CAR_MANAGER.0], |c, _| Some(cheapest_car_stock_price(&mut c.m, CAR_MANAGER) as u32)),
            ("CalcUsedPrice", nfs4rc::f_8001648c, |_, r| vec![CAR_MANAGER.0, id_arg(r)], |c, a| Some(calc_used_price(&mut c.m, CAR_MANAGER, a[1] as i32) as u32)),
            ("PurchaseCar", nfs4rc::f_80016598, |_, r| {
                let (id, color) = (pick(r, 52) as i32 - 1, pick(r, 16) as i32);
                vec![CAR_MANAGER.0, short_arg(r, id), short_arg(r, color), player_arg(r)]
            }, |c, a| Some(purchase_car(&mut c.m, CAR_MANAGER, a[1] as i32, a[2] as i32, a[3] as i32) as u32)),
            ("SellCar", nfs4rc::f_80016674, |_, r| vec![CAR_MANAGER.0, id_arg(r), player_arg(r)], |c, a| Some(sell_car(c, CAR_MANAGER, a[1], a[2]) as u32)),
            ("PurchaseUpgrade", nfs4rc::f_800167f4, |_, r| {
                let upgrades = pick(r, 16) as i32;
                vec![CAR_MANAGER.0, id_arg(r), short_arg(r, upgrades), player_arg(r)]
            }, |c, a| Some(purchase_upgrade(&mut c.m, CAR_MANAGER, a[1] as i32, a[2] as i32, a[3] as i32) as u32)),
            ("RemoveFromPinkSlipsList", nfs4rc::f_80016900, |_, r| vec![CAR_MANAGER.0, id_arg(r), player_arg(r)], |c, a| {
                remove_from_pink_slips_list(&mut c.m, CAR_MANAGER, a[1], a[2]);
                None
            }),
            ("AddToPinkSlipsList", nfs4rc::f_80016a48, |_, r| {
                let (id, color) = (pick(r, 52) as i32 - 1, pick(r, 16) as i32);
                vec![CAR_MANAGER.0, short_arg(r, id), short_arg(r, color), player_arg(r)]
            }, |c, a| {
                add_to_pink_slips_list(&mut c.m, CAR_MANAGER, a[1] as i32, a[2] as i32, a[3] as i32);
                None
            }),
            ("AddUpgradesToPinkSlipsList", nfs4rc::f_80016ac4, |_, r| {
                let upgrades = pick(r, 16) as i32;
                vec![CAR_MANAGER.0, id_arg(r), short_arg(r, upgrades), player_arg(r)]
            }, |c, a| {
                add_upgrades_to_pink_slips_list(&mut c.m, CAR_MANAGER, a[1] as i32, a[2] as i32, a[3] as i32);
                None
            }),
            ("LoadCars", nfs4rc::f_80016b9c, |_, r| vec![CAR_MANAGER.0, SAVE, player_arg(r)], |c, a| {
                load_cars(c, CAR_MANAGER, a[1], a[2]);
                None
            }),
            ("SaveCars", nfs4rc::f_80016c38, |_, _| vec![CAR_MANAGER.0, SAVE], |c, a| {
                save_cars(c, CAR_MANAGER, a[1]);
                None
            }),
            ("LoadPinkSlipsCars", nfs4rc::f_80016cb0, |_, r| vec![CAR_MANAGER.0, SAVE, player_arg(r)], |c, a| {
                load_pink_slips_cars(c, CAR_MANAGER, a[1], a[2]);
                None
            }),
            ("SavePinkSlipsCars", nfs4rc::f_80016d54, |_, r| {
                let remove = if pick(r, 3) == 0 { short_arg(r, -1) } else { id_arg(r) };
                vec![CAR_MANAGER.0, SAVE, player_arg(r), remove]
            }, |c, a| {
                save_pink_slips_cars(c, CAR_MANAGER, a[1], a[2], a[3]);
                None
            }),
            ("SetClassAvailable", nfs4rc::f_80016eb0, |_, r| vec![CAR_MANAGER.0, pick(r, 10), pick(r, 3)], |c, a| {
                set_class_available(&mut c.m, CAR_MANAGER, a[1], a[2]);
                None
            }),
            ("SetCarAvailable", nfs4rc::f_80016f34, |_, r| vec![CAR_MANAGER.0, pick(r, 50), pick(r, 3)], |c, a| {
                set_car_available(&mut c.m, CAR_MANAGER, a[1], a[2]);
                None
            }),
            ("SetClassViewable", nfs4rc::f_80016f50, |_, r| vec![CAR_MANAGER.0, pick(r, 10), pick(r, 3)], |c, a| {
                set_class_viewable(&mut c.m, CAR_MANAGER, a[1], a[2]);
                None
            }),
            ("SetCarViewable", nfs4rc::f_80016fbc, |_, r| vec![CAR_MANAGER.0, pick(r, 50), pick(r, 3)], |c, a| {
                set_car_viewable(&mut c.m, CAR_MANAGER, a[1], a[2]);
                None
            }),
            ("GetStockCar", nfs4rc::f_80016fd8, |_, r| vec![CAR_MANAGER.0, id_arg(r), OUT], |c, a| {
                get_stock_car(c, CAR_MANAGER, a[1], a[2]);
                None
            }),
            ("GetGarageCar", nfs4rc::f_80017094, |_, r| vec![CAR_MANAGER.0, id_arg(r), OUT, player_arg(r)], |c, a| {
                get_garage_car(c, CAR_MANAGER, a[1], a[2], a[3]);
                None
            }),
            ("GetPinkSlipsCar", nfs4rc::f_800171a0, |_, r| vec![CAR_MANAGER.0, id_arg(r), OUT, player_arg(r)], |c, a| {
                get_pink_slips_car(c, CAR_MANAGER, a[1], a[2], a[3]);
                None
            }),
            ("ReleaseDescription", nfs4rc::f_8001743c, |_, _| vec![CAR_MANAGER.0], |c, _| {
                release_description(c, CAR_MANAGER);
                None
            }),
            ("GetNumOwnedCars", nfs4rc::f_8001747c, |_, r| vec![CAR_MANAGER.0, player_arg(r)], |c, a| Some(get_num_owned_cars(&mut c.m, CAR_MANAGER, a[1]) as u32)),
            ("GetNumTourneyCars", nfs4rc::f_800174c0, |_, r| vec![CAR_MANAGER.0, player_arg(r)], |c, a| Some(get_num_tourney_cars(c, CAR_MANAGER, a[1]) as u32)),
            ("GetNumPinkSlipsCars", nfs4rc::f_80017568, |_, r| vec![CAR_MANAGER.0, player_arg(r)], |c, a| Some(get_num_pink_slips_cars(&mut c.m, CAR_MANAGER, a[1]) as u32)),
            ("GetClassList", nfs4rc::f_800175ac, |_, r| {
                let (class, max) = (pick(r, 10), pick(r, 40) as i32 - 2);
                vec![CAR_MANAGER.0, class, short_arg(r, max), LIST]
            }, |c, a| Some(get_class_list(&mut c.m, CAR_MANAGER, a[1], a[2], a[3]) as u32)),
            ("InitializeIngameCarList", nfs4rc::f_800176c0, |_, _| vec![CAR_MANAGER.0], |c, _| {
                initialize_ingame_car_list(&mut c.m);
                None
            }),
            ("IsCarAnAddedModel", nfs4rc::f_80017740, |_, _| vec![CAR_MANAGER.0, MODEL, COLOR], |c, a| Some(is_car_an_added_model(&mut c.m, CAR_MANAGER, a[1], a[2]) as u32)),
            ("AddCarToIngameList", nfs4rc::f_800177c0, |_, _| vec![CAR_MANAGER.0, MODEL, COLOR], |c, a| {
                add_car_to_ingame_list(&mut c.m, CAR_MANAGER, a[1], a[2]);
                None
            }),
            ("FindSimilarCar", nfs4rc::f_80017860, |_, _| vec![CAR_MANAGER.0, MODEL, COLOR], |c, a| Some(find_similar_car(&mut c.m, CAR_MANAGER, a[1], a[2]))),
        ];
        run_cases(cases, 0xFEC0);
    }

    #[test]
    fn list_iterators_fuzz() {
        let cases: &[Case] = &[
            ("tListIteratorCar", nfs4rc::f_80017a14, |_, _| vec![IT_NEW, IT_CAR_VALUES, CAR_MANAGER.0], |c, a| Some(list_iterator_car(c, a[0], a[1], a[2]))),
            ("~tListIteratorCar", nfs4rc::f_80017a70, |_, r| vec![IT_CAR, 2 * pick(r, 2)], |c, a| {
                c.r[4] = a[0];
                c.r[5] = a[1];
                abi_list_iterator_car_dtor(c);
                None
            }),
            ("Value", nfs4rc::f_80017a98, |_, r| vec![IT_CAR, menu_player_arg(r)], |c, a| Some(list_iterator_car_value(&mut c.m, a[0], a[1]))),
            ("TextValue", nfs4rc::f_80017ac4, |_, r| vec![IT_CAR, menu_player_arg(r)], |c, a| Some(list_iterator_car_text_value(c, a[0], a[1]))),
            ("AdjustPosition", nfs4rc::f_80017be4, |c, r| walk_args(c, r, None), |c, a| {
                adjust_position(c, a[0], a[1], a[2]);
                None
            }),
            ("Increment", nfs4rc::f_800181d4, |c, r| walk_args(c, r, Some(1)), |c, a| {
                list_iterator_car_increment(c, a[0], a[1]);
                None
            }),
            ("Decrement", nfs4rc::f_800181f4, |c, r| walk_args(c, r, Some(u32::MAX)), |c, a| {
                list_iterator_car_decrement(c, a[0], a[1]);
                None
            }),
            ("ValidCar", nfs4rc::f_80018214, |_, r| {
                let high = if pick(r, 6) == 0 { r.next() as u32 & 0xFFFF_FF00 } else { 0 };
                vec![IT_CAR, menu_player_arg(r), pick(r, CARS + 34) | high]
            }, |c, a| Some(valid_car(c, a[0], a[1], a[2]))),
            ("tListIteratorCarColor", nfs4rc::f_800185e0, |_, _| vec![IT_NEW, IT_COLOR_VALUES, IT_COLOR_PLAYER, IT_COLOR_CARS, 48, CAR_MANAGER.0], |c, a| {
                // (the fifth and sixth arguments where the original's caller puts them)
                let sp = c.r[29];
                c.m.w32(sp + 16, a[4]);
                c.m.w32(sp + 20, a[5]);
                Some(list_iterator_car_color(c, a[0], a[1], a[2], a[3], a[4], a[5]))
            }),
            ("~tListIteratorCarColor", nfs4rc::f_80018660, |_, r| vec![IT_COLOR, 2 * pick(r, 2)], |c, a| {
                c.r[4] = a[0];
                c.r[5] = a[1];
                abi_list_iterator_car_color_dtor(c);
                None
            }),
            ("Color::Value", nfs4rc::f_80018688, |_, _| vec![IT_COLOR], |c, a| Some(list_iterator_car_color_value(&mut c.m, a[0]))),
            ("Color::TextValue", nfs4rc::f_800186e8, |_, _| vec![IT_COLOR], |c, _| {
                abi_list_iterator_car_color_text_value(c);
                Some(c.r[2])
            }),
            ("Color::Increment", nfs4rc::f_800186f0, |_, _| vec![IT_COLOR], |c, a| {
                list_iterator_car_color_increment(&mut c.m, a[0]);
                None
            }),
            ("Color::Decrement", nfs4rc::f_80018788, |_, _| vec![IT_COLOR], |c, a| {
                list_iterator_car_color_decrement(&mut c.m, a[0]);
                None
            }),
        ];
        run_cases(cases, 0xFEC1);
    }

    /// The functions that call out leave the stack as the original does (their callees'
    /// frames, the registers they save, the locals).
    #[test]
    fn fecars_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        type StackCase = (&'static str, u32, fn(&mut Cpu), Args);
        let cases: &[StackCase] = &[
            ("SellCar", 0x8001_6674, nfs4rc::f_80016674, |_, r| vec![CAR_MANAGER.0, id_arg(r), player_arg(r)]),
            ("LoadCars", 0x8001_6b9c, nfs4rc::f_80016b9c, |_, r| vec![CAR_MANAGER.0, SAVE, player_arg(r)]),
            ("SaveCars", 0x8001_6c38, nfs4rc::f_80016c38, |_, _| vec![CAR_MANAGER.0, SAVE]),
            ("LoadPinkSlipsCars", 0x8001_6cb0, nfs4rc::f_80016cb0, |_, r| vec![CAR_MANAGER.0, SAVE, player_arg(r)]),
            ("SavePinkSlipsCars", 0x8001_6d54, nfs4rc::f_80016d54, |_, r| {
                let remove = if pick(r, 3) == 0 { u32::MAX } else { id_arg(r) };
                vec![CAR_MANAGER.0, SAVE, player_arg(r), remove]
            }),
            ("GetStockCar", 0x8001_6fd8, nfs4rc::f_80016fd8, |_, r| vec![CAR_MANAGER.0, id_arg(r), OUT]),
            ("GetGarageCar", 0x8001_7094, nfs4rc::f_80017094, |_, r| vec![CAR_MANAGER.0, id_arg(r), OUT, player_arg(r)]),
            ("GetPinkSlipsCar", 0x8001_71a0, nfs4rc::f_800171a0, |_, r| vec![CAR_MANAGER.0, id_arg(r), OUT, player_arg(r)]),
            ("ReleaseDescription", 0x8001_743c, nfs4rc::f_8001743c, |_, _| vec![CAR_MANAGER.0]),
            ("GetNumTourneyCars", 0x8001_74c0, nfs4rc::f_800174c0, |_, r| vec![CAR_MANAGER.0, player_arg(r)]),
            ("tListIteratorCar", 0x8001_7a14, nfs4rc::f_80017a14, |_, _| vec![IT_NEW, IT_CAR_VALUES, CAR_MANAGER.0]),
            ("~tListIteratorCar", 0x8001_7a70, nfs4rc::f_80017a70, |_, r| vec![IT_CAR, 2 * pick(r, 2)]),
            ("TextValue", 0x8001_7ac4, nfs4rc::f_80017ac4, |_, r| vec![IT_CAR, menu_player_arg(r)]),
            ("AdjustPosition", 0x8001_7be4, nfs4rc::f_80017be4, |c, r| walk_args(c, r, None)),
            ("Increment", 0x8001_81d4, nfs4rc::f_800181d4, |c, r| walk_args(c, r, Some(1))),
            ("Decrement", 0x8001_81f4, nfs4rc::f_800181f4, |c, r| walk_args(c, r, Some(u32::MAX))),
            ("ValidCar", 0x8001_8214, nfs4rc::f_80018214, |_, r| vec![IT_CAR, menu_player_arg(r), pick(r, CARS + 34)]),
            ("tListIteratorCarColor", 0x8001_85e0, nfs4rc::f_800185e0, |c, _| {
                // (the fifth and sixth arguments on the caller's stack)
                c.m.w32(TOP + 16, 48);
                c.m.w32(TOP + 20, CAR_MANAGER.0);
                vec![IT_NEW, IT_COLOR_VALUES, IT_COLOR_PLAYER, IT_COLOR_CARS]
            }),
            ("~tListIteratorCarColor", 0x8001_8660, nfs4rc::f_80018660, |_, r| vec![IT_COLOR, 2 * pick(r, 2)]),
        ];
        for (k, &(name, addr, original, args)) in cases.iter().enumerate() {
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 300, 0xFEC5 + k as u64, shuffle, &args);
        }
    }
}
