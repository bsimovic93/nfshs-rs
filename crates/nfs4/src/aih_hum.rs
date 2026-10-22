//! AIH_HUM.CPP: `AIHigh_Human`, a human racer: the police's handling of it, always active.

use crate::abi::{self, Saved};
use crate::aihigh::{self, VT_BASIC_PERP, VT_HUMAN};
use crate::types::{AIHighHuman, AIStateBase, Car};
use ps1rt::Cpu;

/// `AIHigh_Human::HighExecute` (AIH_HUM.CPP, 0x80063a60, virtual): the police, its car
/// active, its state.
pub fn high_execute(c: &mut Cpu, this: AIHighHuman) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let player = this.ai_high_player();
        r.enter(c, 0x8006_32b4, 0x8006_3a74, |c| crate::aih_play::handle_cops(c, player));
        let m = &mut c.m;
        Car(m.r32(this.0)).n().set_active(m, 1);
        let state = AIStateBase(m.r32(this.0 + 4));
        r.enter(c, 0x8007_0244, 0x8006_3a8c, |c| crate::aistate::state_execute(c, state));
    })
}

pub fn abi_high_execute(c: &mut Cpu) {
    high_execute(c, AIHighHuman(c.r[4]));
}

/// `AIHigh_Human::AIHigh_Human(Car_tObj*)` (AIH_HUM.CPP, 0x80063a9c).
pub fn construct(c: &mut Cpu, this: AIHighHuman, car: Car) -> AIHighHuman {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        r.enter(c, 0x8006_30b0, 0x8006_3ab0, |c| crate::aih_play::construct(c, this.ai_high_player(), car));
        aihigh::set_vt(&mut c.m, this.0, VT_HUMAN);
        this
    })
}

pub fn abi_construct(c: &mut Cpu) {
    c.r[2] = construct(c, AIHighHuman(c.r[4]), Car(c.r[5])).0;
}

/// `AIHigh_Human::~AIHigh_Human` (AIHIGH.H, 0x80063ad0).
pub fn abi_destruct(c: &mut Cpu) {
    aihigh::destruct_to_base(c, c.r[4], c.r[5], Some(VT_BASIC_PERP), 0x8006_3ae8);
}
