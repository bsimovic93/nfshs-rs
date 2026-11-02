//! SCREENAUDIO.CPP: the audio options' screen (`tScreenAudio`). It plays the chosen song, sets
//! the music's and the narration's levels, and switches the stereo mode. On each level's slider
//! a test sound (a sound effect, the engine, the narration, the ambience) is played again
//! whenever the last ends, panned about for the stereo test, and the slider's percentage is
//! faded in beside it. It also draws 32 pictures.
//!
//! Written as FEMENU.CPP is (see there): typed views, calls through `crate::call`.
//! `AudioMus_PlaySong` and `PSXDrawSquare` are the host's (called by address).

use crate::call::{call, call_addr, with_buffer};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SPRINTF, TEXT_PIXELS, TEXT_WORD};
use crate::globals::{FRONT_END, SONGS_SCREEN};
use crate::types::{MenuItemLeftRightAudioSlider, MenuItemSlidingMenu, OptionsMenu, Screen};
use crate::{fefades, femenuoptions, fescreen, fetextrender};
use ps1rt::{Cpu, Mem};

/// The screen: a `tScreen`, then +102 the stereo mode set, +104 the test sound (0 none; 1..4 its
/// pan's step), +108 its handle, +112 the song playing (-1: none), +114 the audio mode, +116 the
/// song chosen, +120 the songs (64 bytes each, +8 the file).
const MODE_SET: u32 = 102;
const TEST: u32 = 104;
const HANDLE: u32 = 108;
const PLAYING: u32 = 112;
const AUDIO_MODE: u32 = 114;
const CHOSEN: u32 = 116;
const SONGS: u32 = 120;
const SONG_SIZE: u32 = 64;
const SONG_FILE: u32 = 8;

/// In the menus' block (`SONGS_SCREEN`'s): the songs' menu on (`+10068`), the sliding menu and
/// its inside (`+10000`, `+10072`), the options menu (`+10208`: +8 its item, +116 its fade),
/// the level sliders (`+9676`, 56 bytes apart), the foreground's fade (`+10324`).
const SONGS_ON: u32 = 10068;
const SLIDING_MENU: u32 = 10000;
const INSIDE_MENU: u32 = 10072;
const OPTIONS_MENU: u32 = 10208;
const OPTIONS_ITEM: u32 = 8;
const OPTIONS_FADE: u32 = 116;
const SLIDERS: u32 = 9676;
const SLIDER_SIZE: u32 = 56;
const FOREGROUND_FADE: u32 = 10324;
/// The levels set in the options (`frontEnd + 0x341`: music, sound effects, narration, engine,
/// ambience), the stereo mode (`+0x346`: 0 stereo, 1 mono, 2 the stereo test).
const LEVELS: u32 = FRONT_END + 0x341;
const MUSIC: u32 = 0;
const EFFECTS: u32 = 1;
const NARRATION: u32 = 2;
const ENGINE: u32 = 3;
const AMBIENCE: u32 = 4;
const STEREO: u32 = FRONT_END + 0x346;
const MONO: u8 = 1;
const STEREO_TEST: u8 = 2;
/// The levels in use: music, sound effects, engine, narration, ambience; the stereo flag,
/// the 3D sound's.
const MUSIC_LEVEL: u32 = 0x8013_D944;
const SFX_LEVEL: u32 = 0x8013_D948;
const ENGINE_LEVEL: u32 = 0x8013_D94C;
const NARRATION_LEVEL: u32 = 0x8013_D950;
const AMBIENT_LEVEL: u32 = 0x8013_D954;
const STEREO_MODE: u32 = 0x8013_D968;
const DIRECT_3D: u32 = 0x8013_E7E4;
/// The sound system's options (`SNDSYS_getopts`' 64 bytes: +16 the output, 1 mono or 2 stereo).
const OPTS_SIZE: u32 = 64;
const OPTS_OUTPUT: u32 = 16;
/// The narration's still speaking (`FeAudio`'s +16), the sound banks (`gSndBnk`).
const SPEAKING: u32 = 0x8005_19E0;
const SOUND_BANKS: u32 = 0x8010_FB6C;
/// The test sounds: a sound effect's, the ambience's, the engine's (one of 6 from 41), the
/// narration (none: speech instead).
const EFFECT_SOUND: u32 = 31;
const AMBIENCE_SOUND: u32 = 30;
const ENGINE_SOUNDS: i32 = 41;
const SPEECH: u32 = u32::MAX;
/// The last percentage shown and its text's fade (0 shown .. 128 gone).
const PERCENTAGE: u32 = 0x8005_2D70;
const PERCENTAGE_FADE: u32 = 0x8005_2D74;
/// The colours' table and the percentage's colour's index; "%d%%" and where it goes (text 477's
/// place); "zAudio" (52 shapes), "LOADING".
const COLORS: u32 = 0x8005_1B50;
const PERCENTAGE_COLOR: u32 = 0x8005_1AC9;
const PERCENTAGE_FORMAT: u32 = 0x8001_20DC;
const PERCENTAGE_PLACE: u32 = 477;
const AUDIO_NAME: u32 = 0x8001_20FC;
const SHAPES: u16 = 52;
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

/// A level set in the options.
fn level(m: &mut Mem, k: u32) -> u32 {
    m.r8(LEVELS + k) as u32
}

/// The music's level from the options' (`* 35 / 64`).
fn music_level(m: &mut Mem) -> u32 {
    (level(m, MUSIC) * 35) >> 6
}

/// The options menu.
fn options_menu(m: &mut Mem) -> u32 {
    m.r32(SONGS_SCREEN) + OPTIONS_MENU
}

/// `tScreenAudio::PlaySound` (SCREENAUDIO.CPP, 0x80045e10). The chosen song is played (when the
/// songs' menu is on and it changed); the music's and the narration's levels are set; a new
/// stereo mode is set in the sound system. On a level's slider (items 1..5) its test sound is
/// played again once the last is over (and the narration quiet), panned left, centre, right in
/// turn for the stereo test, at the slider's level. Elsewhere the test sound is stopped.
pub fn play_sound(c: &mut Cpu, this: Screen) {
    let this = this.0;
    let menus = c.m.r32(SONGS_SCREEN);
    if c.m.r32(menus + SONGS_ON) != 0 && lh(&mut c.m, this + PLAYING) != lh(&mut c.m, this + CHOSEN) {
        call(c, STOP_SONG, &[10]);
        let m = &mut c.m;
        let song = m.r32(this + SONGS).wrapping_add((lh(m, this + CHOSEN) as u32).wrapping_mul(SONG_SIZE));
        let file = m.r32(song + SONG_FILE);
        call_addr(c, PLAY_SONG, &[file]);
        let v = c.m.r16(this + CHOSEN);
        c.m.w16(this + PLAYING, v);
    }
    let volume = music_level(&mut c.m);
    call(c, MUSIC_VOLUME, &[volume]);
    let m = &mut c.m;
    let v = level(m, NARRATION);
    m.w32(NARRATION_LEVEL, v);
    if m.r8(STEREO) != m.r8(this + MODE_SET) {
        set_stereo_mode(c, this);
    }
    let m = &mut c.m;
    let options = options_menu(m);
    let item = lh(m, options + OPTIONS_ITEM);
    if !(1..6).contains(&item) {
        if lh(&mut c.m, this + TEST) != 0 {
            let m = &mut c.m;
            let handle = m.r32(this + HANDLE);
            let v = level(m, EFFECTS);
            m.w32(SFX_LEVEL, v);
            call(c, SND_STOP, &[handle]);
            c.m.w16(this + TEST, 0);
        }
        return;
    }
    // the slider's test sound, at its level (the sound effects' level meanwhile)
    let (which, sound) = match item {
        1 => (EFFECTS, Some(EFFECT_SOUND)),
        2 => (ENGINE, None),
        3 => (NARRATION, Some(SPEECH)),
        4 => (AMBIENCE, Some(AMBIENCE_SOUND)),
        _ => (EFFECTS, None),
    };
    let volume = level(&mut c.m, which);
    let sound = match sound {
        Some(s) => s,
        None => (ENGINE_SOUNDS + (call(c, RAND, &[]) as i32).wrapping_rem(6)) as u32,
    };
    let m = &mut c.m;
    m.w32(SFX_LEVEL, volume);
    let over = if lh(m, this + TEST) == 0 {
        let step = if m.r8(STEREO) == MONO { 2 } else { 1 };
        m.w16(this + TEST, step);
        true
    } else {
        let handle = m.r32(this + HANDLE);
        call(c, SND_OVER, &[handle]) != 0
    };
    let m = &mut c.m;
    if over && m.r16(SPEAKING) == 0 && sound != 0 {
        // the pan (left, centre, right, behind), and the stereo test's next step
        let stereo = m.r8(STEREO);
        let (pan, next) = match lh(m, this + TEST) {
            1 => (0xC000, Some(if stereo == STEREO_TEST { 2 } else { 3 })),
            2 => (0, Some(if stereo == MONO { 2 } else { 3 })),
            3 => (16383, Some(if stereo == STEREO_TEST { 4 } else { 1 })),
            4 => (0x8000, Some(1)),
            _ => (0, None),
        };
        if let Some(next) = next {
            m.w16(this + TEST, next);
        }
        if sound == SPEECH {
            call(c, PLAY_SPEECH, &[2, 3]);
            c.m.w32(this + HANDLE, 0);
        } else {
            let bank = m.r32(SOUND_BANKS);
            let handle = call(c, PLAY_SOUND, &[bank, sound, pan, volume, 64]);
            c.m.w32(this + HANDLE, handle);
        }
    }
    let v = level(&mut c.m, EFFECTS);
    c.m.w32(SFX_LEVEL, v);
}

/// The options' stereo mode set in the sound system (mono, stereo, or the stereo test's 3D
/// sound), the test sound's pan started over.
fn set_stereo_mode(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    m.w32(STEREO_MODE, 1);
    m.w32(DIRECT_3D, 0);
    m.w16(this + TEST, 1);
    with_buffer(c, OPTS_SIZE, |c, opts| {
        call(c, GET_OPTS, &[opts]);
        let m = &mut c.m;
        match m.r8(STEREO) {
            MONO => {
                m.w8(opts + OPTS_OUTPUT, 1);
                m.w32(STEREO_MODE, 0);
                m.w16(this + TEST, 2);
            }
            0 => m.w8(opts + OPTS_OUTPUT, 2),
            _ => {
                m.w8(opts + OPTS_OUTPUT, 2);
                m.w32(DIRECT_3D, 1);
            }
        }
        call(c, SET_OPTS, &[opts]);
    });
    let v = c.m.r8(STEREO);
    c.m.w8(this + MODE_SET, v);
}

pub fn abi_play_sound(c: &mut Cpu) {
    play_sound(c, Screen(c.r[4]));
}

/// `tScreenAudio::DrawForeground` (SCREENAUDIO.CPP, 0x800461b0): shapes 48 to 51 at half the
/// foreground's fade (0..128); "LOADING" over a bar while it is 100 or more.
pub fn draw_foreground(c: &mut Cpu) {
    let menus = c.m.r32(SONGS_SCREEN);
    let bright = ((c.m.r32(menus + FOREGROUND_FADE) as i32 >> 1) as i16).clamp(0, 128) as u32;
    for k in 0..4 {
        call(c, DRAW_SHAPE_EXTENDED, &[k + 48, 1, 0, 0, bright, 0, 0]);
    }
    if bright >= 100 {
        fetextrender::menu_text_positioned_justify(c, LOADING_TEXT, 480, 220, 1, 1, 4);
        let w = call(c, TEXT_WORD, &[LOADING_TEXT]);
        let px = call(c, TEXT_PIXELS, &[w]);
        call_addr(c, DRAW_SQUARE, &[0, 480, 220, 0u32.wrapping_sub(px).wrapping_sub(5), 7]);
    }
}

pub fn abi_draw_foreground(c: &mut Cpu) {
    draw_foreground(c);
}

/// `tScreenAudio::DrawBackground` (SCREENAUDIO.CPP, 0x800462c0): the sounds; on a level's
/// slider its percentage (kept once off them) faded in beside it, out elsewhere or while the
/// options menu moves; 32 pictures at half the options menu's fade.
pub fn draw_background(c: &mut Cpu, this: Screen) {
    play_sound(c, this);
    let m = &mut c.m;
    let options = options_menu(m);
    let bright = ((m.r32(options + OPTIONS_FADE) as i32 >> 1) as i16).min(128) as i32 as u32;
    let item = lh(m, options + OPTIONS_ITEM) as u32;
    let shown = if item < 5 {
        let slider = m.r32(SONGS_SCREEN) + SLIDERS + SLIDER_SIZE * item;
        femenuoptions::audio_slider_percentage(c, MenuItemLeftRightAudioSlider(slider))
    } else {
        u32::MAX
    };
    if (shown as i32) >= 0 {
        c.m.w32(PERCENTAGE, shown);
    }
    if (shown as i32) >= 0 || (c.m.r32(PERCENTAGE) as i32) >= 0 {
        // out when off the sliders or while the options menu moves, else in
        let out = shown == u32::MAX || {
            let options = options_menu(&mut c.m);
            femenuoptions::options_menu_transition_is_finished(c, OptionsMenu(options)) != 1
        };
        let m = &mut c.m;
        let fade = m.r32(PERCENTAGE_FADE).wrapping_add(if out { 4 } else { (-4i32) as u32 });
        let fade = (fade as i32).clamp(0, 128) as u32;
        let k = m.r8(PERCENTAGE_COLOR) as u32;
        let base = m.r32(COLORS + 4 * k);
        m.w32(PERCENTAGE_FADE, fade);
        let color = fefades::fade(fefades::fade(base, 0, bright), 0, fade);
        let value = if (shown as i32) < 0 { m.r32(PERCENTAGE) } else { shown };
        with_buffer(c, 32, |c, text| {
            call(c, SPRINTF, &[text, PERCENTAGE_FORMAT, value]);
            if c.m.r32(PERCENTAGE_FADE) != 128 {
                let x = sx(call(c, WORD_X, &[PERCENTAGE_PLACE]));
                let y = sx(call(c, WORD_Y, &[PERCENTAGE_PLACE]));
                fetextrender::full_text_rgb(c, text, x, y, color, 0, 1);
            }
        });
    }
    // the pictures
    for k in 0..32 {
        call(c, DRAW_SHAPE_EXTENDED, &[k + 6, 1, 0, 0, bright, 0, 0]);
    }
}

pub fn abi_draw_background(c: &mut Cpu) {
    draw_background(c, Screen(c.r[4]));
}

/// `tScreenAudio::GetShapeInfo` (SCREENAUDIO.CPP, 0x80046528): 52 shapes of "zAudio", none
/// swapped.
pub fn get_shape_info(m: &mut Mem, permanent: u32, swap: u32, name: u32, swap_name: u32) {
    m.w16(swap, 0);
    m.w32(swap_name, 0);
    m.w16(permanent, SHAPES);
    m.w32(name, AUDIO_NAME);
}

pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = crate::abi::arg(c, 4);
    get_shape_info(&mut c.m, c.r[5], c.r[6], c.r[7], swap_name);
}

/// `tScreenAudio::tScreenAudio` (SCREENAUDIO.CPP, 0x8004654c): a screen with the audio
/// screen's vtable, the first song chosen, mode 0, no songs.
pub fn screen_audio(c: &mut Cpu, this: Screen) -> Screen {
    fescreen::screen(c, this);
    let m = &mut c.m;
    this.set_vtable(m, AUDIO_VT);
    m.w16(this.0 + CHOSEN, 0);
    m.w8(this.0 + AUDIO_MODE, 0);
    m.w32(this.0 + SONGS, 0);
    this
}

pub fn abi_screen_audio(c: &mut Cpu) {
    c.r[2] = screen_audio(c, Screen(c.r[4])).0;
}

/// `tScreenAudio::Initialize` (SCREENAUDIO.CPP, 0x8004658c): no song playing; the sliding
/// menu's inside set; the screen's files read; no test sound; the stereo mode the options'.
pub fn initialize(c: &mut Cpu, this: Screen) {
    let menus = c.m.r32(SONGS_SCREEN);
    c.m.w16(this.0 + PLAYING, 0xFFFF);
    femenuoptions::sliding_menu_set_menu(&mut c.m, MenuItemSlidingMenu(menus + SLIDING_MENU), 1, menus + INSIDE_MENU);
    fescreen::initialize(c, this);
    let m = &mut c.m;
    let v = m.r8(STEREO);
    m.w16(this.0 + TEST, 0);
    m.w32(this.0 + HANDLE, 0);
    m.w32(this.0 + SONGS, 0);
    m.w8(this.0 + MODE_SET, v);
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(c, Screen(c.r[4]));
}

/// `tScreenAudio::Cleanup` (SCREENAUDIO.CPP, 0x800465ec): the test sound stopped; the levels
/// in use the options'; the screen's; the narration waited out.
pub fn cleanup(c: &mut Cpu, this: Screen) {
    if lh(&mut c.m, this.0 + TEST) != 0 {
        let handle = c.m.r32(this.0 + HANDLE);
        call(c, SND_STOP, &[handle]);
        c.m.w16(this.0 + TEST, 0);
    }
    let volume = music_level(&mut c.m);
    call(c, MUSIC_VOLUME, &[volume]);
    let m = &mut c.m;
    let volume = music_level(m);
    for (k, at) in [(MUSIC, MUSIC_LEVEL), (EFFECTS, SFX_LEVEL), (NARRATION, NARRATION_LEVEL), (ENGINE, ENGINE_LEVEL), (AMBIENCE, AMBIENT_LEVEL)] {
        let v = level(m, k);
        m.w32(at, v);
    }
    call(c, MUSIC_VOLUME, &[volume]);
    fescreen::cleanup(c, this);
    while c.m.r16(SPEAKING) != 0 {
        call(c, SYSTEM_TASK, &[0]);
    }
}

pub fn abi_cleanup(c: &mut Cpu) {
    cleanup(c, Screen(c.r[4]));
}

/// `tScreenAudio::~tScreenAudio` (SCREENAUDIO.H, 0x800466e4): the screen's.
pub fn abi_destructor(c: &mut Cpu) {
    fescreen::screen_destructor(c, Screen(c.r[4]), c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

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

}
