//! SCREENAUDIO.CPP: the audio options' screen (`tScreenAudio`): the chosen song played, the
//! music's level and the narration's set, the stereo mode switched; on each level's slider a
//! test sound (a sound effect, the engine, the narration, the ambience) played again whenever
//! the last ends, panned about for the stereo test; the slider's percentage faded in beside it;
//! 32 pictures.
//!
//! `AudioMus_PlaySong`, `PSXDrawSquare`: the host's (called by address).

use crate::abi::{self, Saved};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SPRINTF, TEXT_PIXELS, TEXT_WORD};
use crate::globals::{FRONT_END, SONGS_SCREEN};
use crate::{fefades, femenuoptions, fescreen, fetextrender};
use ps1rt::{Cpu, Mem};

// The screen: a `tScreen`, then +102 the stereo mode set, +104 the test sound (0 none; 1..4 its
// pan's step), +108 its handle, +112 the song playing (-1: none), +114 the audio mode, +116 the
// song chosen, +120 the songs (64 bytes each, +8 the file).
const MODE_SET: u32 = 102;
const TEST: u32 = 104;
const HANDLE: u32 = 108;
const PLAYING: u32 = 112;
const AUDIO_MODE: u32 = 114;
const CHOSEN: u32 = 116;
const SONGS: u32 = 120;

/// In the menus' block (`SONGS_SCREEN`'s): the songs' menu on (`+10068`), the sliding menu and
/// its inside (`+10000`, `+10072`), the options menu (`+10208`: +8 its item, +116 its fade),
/// the level sliders (`+9676`, 56 bytes apart), the foreground's fade (`+10324`).
const SONGS_ON: u32 = 10068;
const SLIDING_MENU: u32 = 10000;
const INSIDE_MENU: u32 = 10072;
const OPTIONS_MENU: u32 = 10208;
const SLIDERS: u32 = 9676;
const SLIDER_SIZE: u32 = 56;
const FOREGROUND_FADE: u32 = 10324;
/// The levels set in the options (`frontEnd + 0x341`: music, sound effects, narration, engine,
/// ambience), the stereo mode (`+0x346`: 1 mono, 2 the stereo test).
const LEVELS: u32 = FRONT_END + 0x341;
const STEREO: u32 = FRONT_END + 0x346;
/// The levels in use: music, sound effects, engine, narration, ambience; the stereo flag,
/// the 3D sound's.
const MUSIC_LEVEL: u32 = 0x8013_D944;
const SFX_LEVEL: u32 = 0x8013_D948;
const ENGINE_LEVEL: u32 = 0x8013_D94C;
const NARRATION_LEVEL: u32 = 0x8013_D950;
const AMBIENT_LEVEL: u32 = 0x8013_D954;
const STEREO_MODE: u32 = 0x8013_D968;
const DIRECT_3D: u32 = 0x8013_E7E4;
/// The narration's still speaking (`FeAudio`'s +16), the sound banks (`gSndBnk`).
const SPEAKING: u32 = 0x8005_19E0;
const SOUND_BANKS: u32 = 0x8010_FB6C;
/// The last percentage shown and its text's fade (0 shown .. 128 gone).
const PERCENTAGE: u32 = 0x8005_2D70;
const PERCENTAGE_FADE: u32 = 0x8005_2D74;
/// The colours' table and the percentage's colour's index; "%d%%", "zAudio", "LOADING".
const COLORS: u32 = 0x8005_1B50;
const PERCENTAGE_COLOR: u32 = 0x8005_1AC9;
const PERCENTAGE_FORMAT: u32 = 0x8001_20DC;
const AUDIO_NAME: u32 = 0x8001_20FC;
const LOADING_TEXT: u32 = 638;
/// The screen's vtable.
const AUDIO_VT: u32 = 0x8001_2108;

/// The host's: `AudioMus_PlaySong`, `PSXDrawSquare`.
const PLAY_SONG: u32 = 0x8007_B9C4;
const DRAW_SQUARE: u32 = 0x8004_F12C;
/// `AudioMus_StopSong`, the music's level (0x8007bc44), `SNDSYS_getopts`, `SNDSYS_setopts`,
/// `SNDover`, `SNDstop`, `rand`, `FeAudio_AsyncPlaySpeech`, `AudioCmn_PlaySound`,
/// `FeAudio_systemtask`; `TextSys_WordX`, `TextSys_WordY`.
const STOP_SONG: fn(&mut Cpu) = nfs4rc::f_8007b798;
const MUSIC_VOLUME: fn(&mut Cpu) = nfs4rc::f_8007bc44;
const GET_OPTS: fn(&mut Cpu) = nfs4rc::f_800f0b58;
const SET_OPTS: fn(&mut Cpu) = nfs4rc::f_800f0c14;
const SND_OVER: fn(&mut Cpu) = nfs4rc::f_800e8700;
const SND_STOP: fn(&mut Cpu) = nfs4rc::f_800e8a6c;
const RAND: fn(&mut Cpu) = nfs4rc::f_800eb430;
const PLAY_SPEECH: fn(&mut Cpu) = crate::feaudio::async_play_speech_entry;
const PLAY_SOUND: fn(&mut Cpu) = nfs4rc::f_80078d5c;
const SYSTEM_TASK: fn(&mut Cpu) = crate::feaudio::systemtask_entry;
const WORD_X: fn(&mut Cpu) = nfs4rc::f_800b9f58;
const WORD_Y: fn(&mut Cpu) = nfs4rc::f_800b9f78;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

/// A call to a function that is ported too, as FEAPP's.
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len()].copy_from_slice(args);
        f(c);
        c.r[2]
    })
}

/// The music's level from the options' (0..255 to 0..127 or so: `* 35 / 64`).
fn music_level(m: &mut Mem) -> u32 {
    (m.r8(LEVELS) as u32 * 35) >> 6
}

/// `tScreenAudio::PlaySound` (SCREENAUDIO.CPP, 0x80045e10): the chosen song played (when the
/// songs' menu is on and it changed); the music's and the narration's levels; a new stereo mode
/// set in the sound system; on a level's slider (items 1..5) its test sound played again once
/// the last is over (and the narration quiet), panned left, centre, right in turn for the
/// stereo test, at the slider's level; elsewhere the test sound stopped.
pub fn play_sound(c: &mut Cpu, this: u32) {
    abi::frame(c, 112, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let menus = c.m.r32(SONGS_SCREEN);
        if c.m.r32(menus + SONGS_ON) != 0 && lh(&mut c.m, this + PLAYING) != lh(&mut c.m, this + CHOSEN) {
            r.call_leaving(c, STOP_SONG, &[10], 0x8004_5e60);
            let m = &mut c.m;
            let song = m.r32(this + SONGS).wrapping_add((lh(m, this + CHOSEN) as u32) << 6);
            let file = m.r32(song + 8);
            r.call_ptr_leaving(c, PLAY_SONG, &[file], 0x8004_5e7c);
            let v = c.m.r16(this + CHOSEN);
            c.m.w16(this + PLAYING, v);
        }
        r.0[2] = FRONT_END;
        let level = music_level(&mut c.m);
        r.call_leaving(c, MUSIC_VOLUME, &[level], 0x8004_5eb0);
        let m = &mut c.m;
        let v = m.r8(LEVELS + 2) as u32;
        m.w32(NARRATION_LEVEL, v);
        r.0[4] = 0x8014_0000;
        if m.r8(STEREO) != m.r8(this + MODE_SET) {
            let opts = sp + 24;
            r.0[0] = 1;
            r.0[3] = 0x8014_0000;
            m.w32(STEREO_MODE, 1);
            m.w32(DIRECT_3D, 0);
            m.w16(this + TEST, 1);
            r.call_leaving(c, GET_OPTS, &[opts], 0x8004_5eec);
            let m = &mut c.m;
            match m.r8(STEREO) {
                1 => {
                    m.w8(sp + 40, 1);
                    m.w32(STEREO_MODE, 0);
                    m.w16(this + TEST, 2);
                }
                0 => m.w8(sp + 40, 2),
                _ => {
                    m.w8(sp + 40, 2);
                    m.w32(DIRECT_3D, 1);
                }
            }
            r.call_leaving(c, SET_OPTS, &[opts], 0x8004_5f3c);
            let v = c.m.r8(STEREO);
            c.m.w8(this + MODE_SET, v);
        }
        let item = lh(&mut c.m, menus + OPTIONS_MENU + 8);
        if !(item > 0 && item < 6) {
            if lh(&mut c.m, this + TEST) != 0 {
                let m = &mut c.m;
                let handle = m.r32(this + HANDLE);
                let v = m.r8(LEVELS + 1) as u32;
                m.w32(SFX_LEVEL, v);
                r.call_leaving(c, SND_STOP, &[handle], 0x8004_618c);
                c.m.w16(this + TEST, 0);
            }
            return;
        }
        // the slider's test sound: its level (and the sound effects' level meanwhile) and sound
        r.0[3] = 1;
        let m = &mut c.m;
        let (level_at, sound) = match item {
            1 => (LEVELS + 1, Some(31)),
            2 => (LEVELS + 3, None),
            3 => (LEVELS + 2, Some(u32::MAX)),
            4 => (LEVELS + 4, Some(30)),
            _ => (LEVELS + 1, None),
        };
        r.0[2] = m.r8(level_at) as u32;
        r.0[0] = match sound {
            Some(s) => s,
            None => {
                let v = r.call_leaving(c, RAND, &[], 0x8004_5ff8) as i32;
                (v.wrapping_rem(6) + 41) as u32
            }
        };
        let m = &mut c.m;
        m.w32(SFX_LEVEL, r.0[2]);
        if lh(m, this + TEST) == 0 {
            let step = if m.r8(STEREO) == 1 { 2 } else { 1 };
            m.w16(this + TEST, step);
        } else {
            let handle = m.r32(this + HANDLE);
            r.0[3] = r.call_leaving(c, SND_OVER, &[handle], 0x8004_6060);
        }
        let m = &mut c.m;
        if r.0[3] != 0 && m.r16(SPEAKING) == 0 && r.0[0] != 0 {
            // the pan, and the stereo test's next step
            let stereo = m.r8(STEREO);
            let (pan, next) = match lh(m, this + TEST) {
                1 => (0xC000, Some(if stereo == 2 { 2 } else { 3 })),
                2 => (0, Some(if stereo == 1 { 2 } else { 3 })),
                3 => (16383, Some(if stereo == 2 { 4 } else { 1 })),
                4 => (0x8000, Some(1)),
                _ => (0, None),
            };
            if let Some(next) = next {
                m.w16(this + TEST, next);
            }
            if r.0[0] == u32::MAX {
                r.call_leaving(c, PLAY_SPEECH, &[2, 3], 0x8004_6130);
                c.m.w32(this + HANDLE, 0);
            } else {
                let bank = m.r32(SOUND_BANKS);
                let handle = r.call_leaving(c, PLAY_SOUND, &[bank, r.0[0], pan, r.0[2], 64], 0x8004_6150);
                c.m.w32(this + HANDLE, handle);
            }
        }
        let v = c.m.r8(LEVELS + 1) as u32;
        c.m.w32(SFX_LEVEL, v);
    })
}

pub fn abi_play_sound(c: &mut Cpu) {
    play_sound(c, c.r[4]);
}

/// `tScreenAudio::DrawForeground` (SCREENAUDIO.CPP, 0x800461b0): shapes 48 to 51 at half the
/// foreground's fade; "LOADING" while it is 100 or more.
pub fn abi_draw_foreground(c: &mut Cpu) {
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        let menus = c.m.r32(SONGS_SCREEN);
        let half = (c.m.r32(menus + FOREGROUND_FADE) as i32 >> 1) as u32;
        let bright = match half as i16 {
            v if v < 128 && v <= 0 => 0,
            v if v >= 129 => 128,
            _ => half,
        };
        r.0[2] = bright;
        r.0[1] = sx(bright);
        for k in 0..4 {
            r.0[0] = k;
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[k + 48, 1, 0, 0, sx(bright), 0, 0], 0x8004_6234);
        }
        r.0[0] = 4;
        if (sx(bright) as i32) >= 100 {
            r.call_leaving(c, fetextrender::menu_text_positioned_justify_entry, &[LOADING_TEXT, 480, 220, 1, 1, 4], 0x8004_6278);
            let w = r.call_leaving(c, TEXT_WORD, &[LOADING_TEXT], 0x8004_6280);
            let px = r.call_leaving(c, TEXT_PIXELS, &[w], 0x8004_6288);
            r.call_ptr_leaving(c, DRAW_SQUARE, &[0, 480, 220, 0u32.wrapping_sub(px).wrapping_sub(5), 7], 0x8004_62a8);
        }
    })
}

/// `tScreenAudio::DrawBackground` (SCREENAUDIO.CPP, 0x800462c0): the sounds; on a level's
/// slider its percentage (kept once off them) faded in beside it, out elsewhere or while the
/// options menu moves; 32 pictures at half the options menu's fade.
pub fn abi_draw_background(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 312, |c, sp| {
        let mut r = Saved::of(c);
        port(c, &r, 0x8004_5e10, 0x8004_62dc, &[this], abi_play_sound);
        let menus = c.m.r32(SONGS_SCREEN);
        let options = menus + OPTIONS_MENU;
        let v = (c.m.r32(options + 116) as i32 >> 1) as u32;
        r.0[3] = if (v as i16) >= 129 { 128 } else { v };
        let item = c.m.r16(options + 8) as i16 as i32 as u32;
        r.0[0] = u32::MAX;
        if item < 5 {
            let slider = c.m.r32(SONGS_SCREEN) + SLIDERS + SLIDER_SIZE * item;
            r.0[0] = port(c, &r, 0x8001_e8dc, 0x8004_63a0, &[slider], femenuoptions::abi_audio_slider_percentage);
        }
        let shown = r.0[0];
        if (shown as i32) >= 0 {
            c.m.w32(PERCENTAGE, shown);
        }
        if (shown as i32) >= 0 || (c.m.r32(PERCENTAGE) as i32) >= 0 {
            let out = shown == u32::MAX || {
                let menus = c.m.r32(SONGS_SCREEN);
                port(c, &r, 0x8001_c86c, 0x8004_63e0, &[menus + OPTIONS_MENU], femenuoptions::abi_options_menu_transition_is_finished) ^ 1 != 0
            };
            let m = &mut c.m;
            let fade = m.r32(PERCENTAGE_FADE).wrapping_add(if out { 4 } else { (-4i32) as u32 });
            m.w32(PERCENTAGE_FADE, fade);
            r.0[1] = 0x8005_0000;
            let fade = (m.r32(PERCENTAGE_FADE) as i32).min(128).max(0) as u32;
            let k = m.r8(PERCENTAGE_COLOR) as u32;
            let base = m.r32(COLORS + (k << 2));
            m.w32(PERCENTAGE_FADE, fade);
            let color = port(c, &r, 0x8004_bd8c, 0x8004_6460, &[base, 0, sx(r.0[3]), fade], fefades::abi_fade_faded);
            r.0[2] = color;
            let value = if (shown as i32) < 0 { c.m.r32(PERCENTAGE) } else { shown };
            let text = sp + 32;
            r.call_leaving(c, SPRINTF, &[text, PERCENTAGE_FORMAT, value], 0x8004_6484);
            if c.m.r32(PERCENTAGE_FADE) != 128 {
                let x = r.call_leaving(c, WORD_X, &[477], 0x8004_649c);
                r.0[0] = sx(x);
                let y = r.call_leaving(c, WORD_Y, &[477], 0x8004_64ac);
                r.call_leaving(c, fetextrender::full_text_rgb_entry, &[text, sx(x), sx(y), color, 0, 1], 0x8004_64d0);
            }
        }
        // the pictures
        let bright = sx(r.0[3]);
        r.0[1] = bright;
        for k in 0..32 {
            r.0[0] = k;
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[k + 6, 1, 0, 0, bright, 0, 0], 0x8004_64fc);
        }
    })
}

/// `tScreenAudio::GetShapeInfo` (SCREENAUDIO.CPP, 0x80046528): 52 shapes of "zAudio", none
/// swapped.
pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = abi::arg(c, 4);
    let (permanent, swap, name) = (c.r[5], c.r[6], c.r[7]);
    let m = &mut c.m;
    m.w16(swap, 0);
    m.w32(swap_name, 0);
    m.w16(permanent, 52);
    m.w32(name, AUDIO_NAME);
}

/// `tScreenAudio::tScreenAudio` (SCREENAUDIO.CPP, 0x8004654c): a screen with the audio
/// screen's vtable, no song chosen, mode 0, no songs.
pub fn abi_screen_audio(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8002_6040, 0x8004_6560, &[this], fescreen::abi_screen);
        let m = &mut c.m;
        m.w32(this + 96, AUDIO_VT);
        m.w16(this + CHOSEN, 0);
        m.w8(this + AUDIO_MODE, 0);
        m.w32(this + SONGS, 0);
        c.r[2] = this;
    })
}

/// `tScreenAudio::Initialize` (SCREENAUDIO.CPP, 0x8004658c): no song playing; the sliding
/// menu's inside set; the screen's files read; no test sound; the stereo mode the options'.
pub fn abi_initialize(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let menus = c.m.r32(SONGS_SCREEN);
        c.m.w16(this + PLAYING, 0xFFFF);
        port(c, &r, 0x8001_de88, 0x8004_65bc, &[menus + SLIDING_MENU, 1, menus + INSIDE_MENU], femenuoptions::abi_sliding_menu_set_menu);
        port(c, &r, 0x8002_60cc, 0x8004_65c4, &[this], fescreen::abi_initialize);
        let m = &mut c.m;
        let v = m.r8(STEREO);
        m.w16(this + TEST, 0);
        m.w32(this + HANDLE, 0);
        m.w32(this + SONGS, 0);
        m.w8(this + MODE_SET, v);
    })
}

/// `tScreenAudio::Cleanup` (SCREENAUDIO.CPP, 0x800465ec): the test sound stopped; the levels
/// in use the options'; the screen's; the narration waited out.
pub fn abi_cleanup(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        r.0[0] = 0x8011_0000;
        if lh(&mut c.m, this + TEST) != 0 {
            let handle = c.m.r32(this + HANDLE);
            r.call_leaving(c, SND_STOP, &[handle], 0x8004_661c);
            c.m.w16(this + TEST, 0);
        }
        r.0[0] = FRONT_END;
        let level = music_level(&mut c.m);
        r.call_leaving(c, MUSIC_VOLUME, &[level], 0x8004_6644);
        let m = &mut c.m;
        let level = music_level(m);
        for (k, at) in [MUSIC_LEVEL, SFX_LEVEL, NARRATION_LEVEL, ENGINE_LEVEL, AMBIENT_LEVEL].into_iter().enumerate() {
            let v = m.r8(LEVELS + k as u32) as u32;
            m.w32(at, v);
        }
        r.call_leaving(c, MUSIC_VOLUME, &[level], 0x8004_66a0);
        port(c, &r, 0x8002_61c0, 0x8004_66a8, &[this], fescreen::abi_cleanup);
        r.0[0] = 0x8005_19D0;
        while c.m.r16(SPEAKING) != 0 {
            r.call_leaving(c, SYSTEM_TASK, &[0], 0x8004_66c8);
        }
    })
}

/// `tScreenAudio::~tScreenAudio` (SCREENAUDIO.H, 0x800466e4): the screen's.
pub fn abi_destructor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        port(c, &Saved::of(c), 0x8002_6098, 0x8004_66f4, &[this, flags], fescreen::abi_screen_destructor);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, TOP, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

    /// The game's audio screen; its songs (four, each naming a file); the outputs of
    /// `GetShapeInfo`.
    const SCREEN: u32 = 0x8018_06F4;
    const SONG_LIST: u32 = 0x8014_7100;
    const OUT: u32 = 0x8014_7000;

    /// FEMENU's structures, then the screen: the songs' menu on or not, a song playing and one
    /// chosen, the test sound in each step of its pan or none, the stereo mode set or another;
    /// the options' levels and stereo mode; the options menu's item (each slider, others), its
    /// fade and the foreground's about their limits; the narration speaking or not; the last
    /// percentage and its fade about their ends.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        let s = SCREEN;
        let menus = m.r32(SONGS_SCREEN);
        m.w32(menus + SONGS_ON, pick(rng, 2));
        for k in 0..4 {
            m.w32(SONG_LIST + 64 * k + 8, 0x8001_20FC);
        }
        m.w32(s + SONGS, SONG_LIST);
        m.w16(s + PLAYING, [0xFFFF, 0, 1, 2, 3][pick(rng, 5) as usize]);
        m.w16(s + CHOSEN, pick(rng, 4) as u16);
        m.w16(s + TEST, pick(rng, 6) as u16);
        m.w32(s + HANDLE, pick(rng, 4));
        for k in 0..5 {
            m.w8(LEVELS + k, [0, 85, 128, 255, pick(rng, 256)][pick(rng, 5) as usize] as u8);
        }
        m.w8(STEREO, pick(rng, 3) as u8);
        let set = if pick(rng, 2) == 0 { m.r8(STEREO) as u32 } else { pick(rng, 3) };
        m.w8(s + MODE_SET, set as u8);
        m.w16(menus + OPTIONS_MENU + 8, [0, 1, 2, 3, 4, 5, 6, 0xFFFF][pick(rng, 8) as usize]);
        let fade = |rng: &mut Rng| [0, 1, 199, 200, 256, 257, 258, (-2i32) as u32, 0x1_0102, rng.next() as u32][pick(rng, 10) as usize];
        let v = fade(rng);
        m.w32(menus + OPTIONS_MENU + 116, v);
        let v = fade(rng);
        m.w32(menus + FOREGROUND_FADE, v);
        m.w16(SPEAKING, (pick(rng, 4) == 0) as u16);
        m.w32(PERCENTAGE, [u32::MAX, 0, 50, 100][pick(rng, 4) as usize]);
        m.w32(PERCENTAGE_FADE, [0, 4, 124, 128, 132, (-4i32) as u32][pick(rng, 6) as usize]);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tScreenAudio::PlaySound", 0x8004_5e10, nfs4rc::f_80045e10, |_, _| vec![SCREEN]),
            ("tScreenAudio::DrawForeground", 0x8004_61b0, nfs4rc::f_800461b0, |_, _| vec![SCREEN]),
            ("tScreenAudio::DrawBackground", 0x8004_62c0, nfs4rc::f_800462c0, |_, _| vec![SCREEN]),
            ("tScreenAudio::GetShapeInfo", 0x8004_6528, nfs4rc::f_80046528, |_, _| vec![SCREEN, OUT, OUT + 2, OUT + 4, OUT + 8]),
            ("tScreenAudio", 0x8004_654c, nfs4rc::f_8004654c, |_, _| vec![OUT + 0x100]),
            ("tScreenAudio::Initialize", 0x8004_658c, nfs4rc::f_8004658c, |_, _| vec![SCREEN]),
            ("tScreenAudio::Cleanup", 0x8004_65ec, nfs4rc::f_800465ec, |c, _| {
                // (the narration quiet: the test CPUs' sound never ends it)
                c.m.w16(SPEAKING, 0);
                vec![SCREEN]
            }),
            ("~tScreenAudio", 0x8004_66e4, nfs4rc::f_800466e4, |_, r| vec![SCREEN, 2 * pick(r, 2)]),
        ]
    }

    #[test]
    fn screenaudio_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xA0D ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0xA0E + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

    #[test]
    fn screenaudio_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0xA1E + k as u64, shuffle, &args);
        }
    }
}
