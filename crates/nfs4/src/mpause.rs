//! MPAUSE.CPP: the race's pause menu (its classes are PAUSEMENU.CPP's): the menus built once a
//! race (`tPauseMenuDefs`: pause, options, audio, controller, a yes-or-no), each frame's input
//! and drawing while paused, the menu stack of up to six, the audio menu's test sounds and the
//! vibration settings shown only for a pad that has it.
//!
//! Written readable from the start (step 3 of DECOMP.md's plan): the game's data as the
//! original leaves it, the stack and registers free.

use crate::call::{call_addr, call_virtual, with_buffer};
use crate::globals::{GAME_SETUP, GAME_TICKS, PAUSE_PORT_INDEX};
use crate::pausemenu::{
    self as pm, FBUILD_F4, ITEM_DISABLED, MENU_DRAW, MENU_INITIALIZE, MENU_PROCESS_INPUT, MENU_VPTR, UPDATE_NEXT_TIME,
};
use crate::types::{PMenu, PMenuCommand, PMenuItem, PauseMenuDefs};
use ps1rt::{Cpu, Mem};

/// The menus (`gPauseMenuDefs`), the one up (`gPauseCurrentMenu`), the ones gone back to
/// (`gBackList`, `gBackDepth` of them).
pub const DEFS: u32 = 0x8013_E540;
pub const CURRENT_MENU: u32 = 0x8013_F0C8;
const BACK_LIST: u32 = 0x8013_F3C8;
const BACK_DEPTH: u32 = 0x8013_F0C4;
/// The highlight bar moving to the current item: how far it still is from it
/// (`kMovingHighlight`, a short) and its step a frame (the next short).
const MOVING_HIGHLIGHT: u32 = 0x8013_F0C0;
const HIGHLIGHT_STEP: u32 = 0x8013_F0C2;
/// The menu wants drawing this frame (`gMPauseUpdate`); an item was enabled or disabled
/// (`ChangedEnabling`).
const UPDATE: u32 = 0x8013_E53C;
const CHANGED_ENABLING: u32 = 0x8013_E5D0;
/// A command's bit: ask "are you sure?" first (set in one player races and replays' first
/// modes), the command itself in its low byte.
const COMMAND_CONFIRM: u32 = 0x100;

/// The menus' items' places in `tPauseMenuDefs`.
const ITEM_RESTART: u32 = 0x1C;
const ITEM_QUIT_RACE: u32 = 0x40;
const ITEM_FORFEIT_RACE: u32 = 0x50;
const ITEM_MUSIC_VOLUME: u32 = 0x164;
const ITEM_SHOCK_MODE: u32 = 0x248;
const ITEM_SHOCK_IMPACT: u32 = 0x260;
const MENU_AUDIO_SETTINGS: u32 = 0x1C8;
const DEFS_SIZE: u32 = 0x358;

/// The game's setup: the race type (pink slips, and the one that can't restart), the
/// controllers' configurations, the language, the audio mode (stereo, mono, surround).
const RACE_TYPE_PINK_SLIPS: i32 = 6;
const RACE_TYPE_NO_RESTART: i32 = 2;
const CONTROLLER_CONFIG: u32 = 0x8011_4504;
const SHOCK_MODE: u32 = 0x8011_454C;
const SHOCK_IMPACT: u32 = 0x8011_4554;
const LANGUAGE: u32 = 0x8011_4584;
const AUDIO_MODE: u32 = 0x8011_4590;
/// The text ids' lists the choices step through (`InGameSelectListAudioMode`, `SelectListConfig`).
const AUDIO_MODES: u32 = 0x8013_E51C;
const CONFIGS: u32 = 0x8013_E524;
/// The volumes (`gMasterMusicLevel`, `SFX`, `Engine`, `FENarration`, `Ambient`), each 0 to 127.
const MUSIC_LEVEL: u32 = 0x8013_D944;
const SFX_LEVEL: u32 = 0x8013_D948;
const ENGINE_LEVEL: u32 = 0x8013_D94C;
const NARRATION_LEVEL: u32 = 0x8013_D950;
const AMBIENT_LEVEL: u32 = 0x8013_D954;
const LEVEL_MAX: u32 = 127;
/// The sound's stereo (`gStereoMode`) and surround (`Audio_direct3davail`) flags.
const STEREO: u32 = 0x8013_D968;
const SURROUND: u32 = 0x8013_E7E4;
/// The audio menu's test sound: the menu was up last frame; a level's item is current, that
/// level; a sound is playing, its handle, when it started (`GAME_TICKS`).
const WAS_IN_AUDIO_MENU: u32 = 0x8013_E52C;
const TEST_WANTED: u32 = 0x8013_E52D;
const TEST_PLAYING: u32 = 0x8013_E52E;
const TEST_LEVEL: u32 = 0x8013_E530;
const TEST_HANDLE: u32 = 0x8013_E534;
const TEST_STARTED: u32 = 0x8013_E538;
/// The test sound: its bank's (`0x8010fb90`) sound 16, again once the last is over and 193
/// ticks on.
const TEST_BANK: u32 = 0x8010_FB90;
const TEST_SOUND: u32 = 16;
const TEST_INTERVAL: i32 = 193;
/// The replay's mode (`Replay_ReplayMode`): the commands ask to confirm below 2.
const REPLAY_MODE: u32 = 0x8013_E6D8;

/// Functions called: the pad (`Input_Interface(button, edge)`, `PadGetState`), the sound
/// library's options (`SNDSYS_getopts`, `setopts`) and sounds (`SNDover`, `SNDstop`,
/// `AudioCmn_PlaySound`), the music's level (`AudioCmn_MusicLevel`, `AudioMus_Volume`,
/// `AudioMus_AutoVolume(ticks, level)`), the texts (`TextSys_LoadInGame`, `TextSys_UnloadWords`),
/// the heap (`__builtin_new`), the controller's set-up (`InGame_ResetPSXController`),
/// `DrawSync`, the HUD's boxes (`Hud_RenderPauseBox`, hooked).
const INPUT_INTERFACE: u32 = 0x8009_DDDC;
const PAD_GET_STATE: u32 = 0x800F_07FC;
const SNDSYS_GETOPTS: u32 = 0x800F_0B58;
const SNDSYS_SETOPTS: u32 = 0x800F_0C14;
const SND_OVER: u32 = 0x800E_8700;
const SND_STOP: u32 = 0x800E_8A6C;
const PLAY_SOUND: u32 = 0x8007_8D5C;
const MUSIC_LEVEL_OF: u32 = 0x8007_6E60;
const MUSIC_VOLUME: u32 = 0x8007_BC44;
const MUSIC_AUTO_VOLUME: u32 = 0x8007_BE00;
const LOAD_IN_GAME: u32 = 0x800B_9EA4;
const UNLOAD_WORDS: u32 = 0x800B_9F98;
const NEW: u32 = 0x800A_48C0;
const DELETE: u32 = 0x800A_4910;
const RESET_CONTROLLER: u32 = 0x800D_D0D8;
const DRAW_SYNC: u32 = 0x800E_E1D8;
const RENDER_PAUSE_BOX: u32 = 0x800D_9ABC;
/// `PadGetState`'s answer for a pad without vibration.
const PAD_NO_VIBRATION: u32 = 2;

/// The menus' lines: the title's (`TITLE_Y`), the first item's at 106 + its line, 13 apart; the
/// box 160 wide from x 80, the highlight bar in it.
const LINE: i32 = 13;
const FIRST_LINE: i32 = 106;

fn defs(m: &mut Mem) -> PauseMenuDefs {
    PauseMenuDefs(m.r32(DEFS))
}

fn current_menu(m: &mut Mem) -> PMenu {
    PMenu(m.r32(CURRENT_MENU))
}

/// The item at `off` in the menus disabled or enabled (when it isn't already), noted.
fn set_disabled(m: &mut Mem, defs: PauseMenuDefs, off: u32, disabled: bool) {
    let item = PMenuItem(defs.0.wrapping_add(off));
    if pm::is_disabled(m, item) != disabled {
        m.w32(CHANGED_ENABLING, 1);
        let flags = item.flags(m);
        item.set_flags(m, if disabled { flags | ITEM_DISABLED } else { flags & !ITEM_DISABLED });
    }
}

/// `tPauseMenuDefs::tPauseMenuDefs` (MPAUSE.CPP, 0x8009e998): every menu and item, in place.
pub fn defs_ctor(c: &mut Cpu, this: PauseMenuDefs) -> u32 {
    let d = this.0;
    let at = |off: u32| d.wrapping_add(off);
    let m = &mut c.m;
    // the pause menu: Continue, Restart, Options, Quit Race, Forfeit Race (their commands' 1 to 5)
    pm::text(m, PMenuItem(d), 0);
    pm::command_button(m, this.item_continue(), 1, 1);
    pm::command_button(m, this.item_restart(), 2, 2);
    pm::go_to_menu(m, this.item_options(), 3, at(0xE8), 0);
    pm::command_button(m, this.item_quit_race(), 4, 4);
    pm::command_button(m, this.item_forfeit_race(), 5, 5);
    pm::menu(c, this.menu_pause(), &[d, at(0x0C), at(0x1C), at(0x2C), at(0x40), at(0x50), 0]);
    // options: audio, controller
    let m = &mut c.m;
    pm::text(m, this.item_options_title().base(), 6);
    pm::go_to_menu(m, this.item_audio_settings(), 7, at(MENU_AUDIO_SETTINGS), 0);
    pm::go_to_menu(m, this.item_controller_settings(), 8, at(0x278), 0);
    pm::menu(c, this.menu_options(), &[at(0xB4), at(0xC0), at(0xD4), 0]);
    // audio: the mode, then music, effects, narration, engine and ambient levels
    let m = &mut c.m;
    pm::text(m, this.item_audio_settings_title().base(), 10);
    pm::iterator(m, this.iterator_audio_mode(), AUDIO_MODES, AUDIO_MODE);
    pm::choice(m, this.item_audio_settings_audio_mode(), 11, at(0x148));
    pm::slider(m, this.item_audio_settings_music_volume(), 12, MUSIC_LEVEL, LEVEL_MAX);
    pm::slider(m, this.item_audio_settings_fx_volume(), 13, SFX_LEVEL, LEVEL_MAX);
    pm::slider(m, this.item_audio_settings_speech_volume(), 14, NARRATION_LEVEL, LEVEL_MAX);
    pm::slider(m, this.item_audio_settings_engine_volume(), 15, ENGINE_LEVEL, LEVEL_MAX);
    pm::slider(m, this.item_audio_settings_ambient_volume(), 16, AMBIENT_LEVEL, LEVEL_MAX);
    pm::menu(c, this.menu_audio_settings(), &[at(0x13C), at(0x154), at(0x164), at(0x178), at(0x18C), at(0x1A0), at(0x1B4), 0]);
    // the controller (the pausing player's): its configuration, vibration and its strength
    let m = &mut c.m;
    pm::indexed(m, this.iterator_config(), CONFIGS, CONTROLLER_CONFIG, PAUSE_PORT_INDEX);
    pm::text(m, this.item_controller_settings_title().base(), 20);
    pm::choice(m, this.item_controller_config(), 21, at(0x21C));
    pm::slider_indexed(m, this.item_controller_shock_mode(), 27, SHOCK_MODE, LEVEL_MAX, PAUSE_PORT_INDEX);
    pm::slider_indexed(m, this.item_controller_shock_impact(), 28, SHOCK_IMPACT, LEVEL_MAX, PAUSE_PORT_INDEX);
    pm::menu(c, this.menu_controller_config(), &[at(0x22C), at(0x238), at(0x248), at(0x260), 0]);
    // are you sure? No (back), Yes (the command asked about)
    let m = &mut c.m;
    pm::text(m, this.item_confirm_title().base(), 4);
    pm::text(m, this.item_confirm_are_you_sure().base(), 36);
    pm::command_button(m, crate::types::PMenuItemCommandButton(at(0x2E4)), 38, pm::COMMAND_BACK);
    pm::command_button(m, crate::types::PMenuItemCommandButton(at(0x2F4)), 37, pm::COMMAND_BACK);
    pm::menu(c, PMenu(at(0x304)), &[at(0x2CC), at(0x2D8), at(0x2E4), at(0x2F4), 0]);
    d
}

pub fn abi_defs_ctor(c: &mut Cpu) {
    c.r[2] = defs_ctor(c, PauseMenuDefs(c.r[4]));
}

/// `tPauseMenuDefs::~tPauseMenuDefs` (MPAUSE.CPP, 0x8009ecf8): every member's destructor, the
/// last first; freed when `flags & 1`.
pub fn defs_dtor(c: &mut Cpu, this: u32, flags: u32) {
    type Dtor = fn(&mut Cpu, u32, u32);
    let members: [(u32, Dtor); 31] = [
        (0x304, pm::menu_dtor),
        (0x2F4, pm::command_button_dtor),
        (0x2E4, pm::command_button_dtor),
        (0x2D8, pm::text_dtor),
        (0x2CC, pm::text_dtor),
        (0x278, pm::menu_dtor),
        (0x260, pm::slider_indexed_dtor),
        (0x248, pm::slider_indexed_dtor),
        (0x238, pm::choice_dtor),
        (0x22C, pm::text_dtor),
        (0x21C, pm::indexed_dtor),
        (0x1C8, pm::menu_dtor),
        (0x1B4, pm::slider_dtor),
        (0x1A0, pm::slider_dtor),
        (0x18C, pm::slider_dtor),
        (0x178, pm::slider_dtor),
        (0x164, pm::slider_dtor),
        (0x154, pm::choice_dtor),
        (0x148, pm::iterator_dtor),
        (0x13C, pm::text_dtor),
        (0xE8, pm::menu_dtor),
        (0xD4, pm::go_to_menu_dtor),
        (0xC0, pm::go_to_menu_dtor),
        (0xB4, pm::text_dtor),
        (0x60, pm::menu_dtor),
        (0x50, pm::command_button_dtor),
        (0x40, pm::command_button_dtor),
        (0x2C, pm::go_to_menu_dtor),
        (0x1C, pm::command_button_dtor),
        (0x0C, pm::command_button_dtor),
        (0x00, pm::text_dtor),
    ];
    for (off, dtor) in members {
        dtor(c, this.wrapping_add(off), 2);
    }
    if flags & 1 != 0 {
        call_addr(c, DELETE, &[this, 1]);
    }
}

pub fn abi_defs_dtor(c: &mut Cpu) {
    defs_dtor(c, c.r[4], c.r[5]);
}

/// `MPause_MusicLogic` (MPAUSE.CPP, 0x8009eeac): the sound library set to the audio mode chosen;
/// in the audio menu, the music at its level and, on a level's item, that level's test sound
/// played again and again; leaving it, the sound stopped and the music faded back.
pub fn music_logic(c: &mut Cpu, in_audio_menu: u32) {
    with_buffer(c, 64, |c, options| {
        call_addr(c, SNDSYS_GETOPTS, &[options]);
        // (the library's channels: 1 mono, 2 stereo)
        let channels = options + 16;
        let (stereo, surround, from, to) = match c.m.r32(AUDIO_MODE) as i32 {
            0 => (1, 0, 1, 2),
            1 => (0, 0, 2, 1),
            2 => (1, 1, 1, 2),
            _ => return,
        };
        let now = c.m.r8(channels) as u32;
        c.m.w32(STEREO, stereo);
        if now == from {
            c.m.w8(channels, to);
            call_addr(c, SNDSYS_SETOPTS, &[options]);
        }
        c.m.w32(SURROUND, surround);
    });
    if in_audio_menu & 0xFF == 0 {
        if c.m.r8(WAS_IN_AUDIO_MENU) != 0 {
            let handle = c.m.r32(TEST_HANDLE);
            call_addr(c, SND_STOP, &[handle]);
            call_addr(c, MUSIC_AUTO_VOLUME, &[500, 0]);
        }
    } else {
        if c.m.r8(WAS_IN_AUDIO_MENU) == 0 {
            let level = c.m.r32(MUSIC_LEVEL);
            let level = call_addr(c, MUSIC_LEVEL_OF, &[level]);
            call_addr(c, MUSIC_AUTO_VOLUME, &[500, level]);
        }
        let menu = c.m.r32(CURRENT_MENU);
        // (the audio menu's items: 1 the mode, 2 the music, 3 to 6 the others' levels)
        if matches!(c.m.r32(menu), 1 | 2) {
            let level = c.m.r32(MUSIC_LEVEL);
            let level = call_addr(c, MUSIC_LEVEL_OF, &[level]);
            call_addr(c, MUSIC_VOLUME, &[level]);
        }
        let m = &mut c.m;
        let menu = m.r32(CURRENT_MENU);
        m.w8(TEST_WANTED, 0);
        for (item, level) in [(3, SFX_LEVEL), (4, NARRATION_LEVEL), (5, ENGINE_LEVEL), (6, AMBIENT_LEVEL)] {
            if m.r32(menu) == item {
                let v = m.r32(level);
                m.w8(TEST_WANTED, 1);
                m.w32(TEST_LEVEL, v);
            }
        }
        if m.r8(TEST_WANTED) != 0 {
            let over = if m.r8(TEST_PLAYING) == 0 {
                m.w8(TEST_PLAYING, 1);
                true
            } else {
                let handle = m.r32(TEST_HANDLE);
                call_addr(c, SND_OVER, &[handle]) != 0
            };
            let m = &mut c.m;
            if over && (m.r32(GAME_TICKS).wrapping_sub(m.r32(TEST_STARTED)) as i32) >= TEST_INTERVAL {
                let (bank, now, level) = (m.r32(TEST_BANK), m.r32(GAME_TICKS), m.r32(TEST_LEVEL));
                m.w32(TEST_STARTED, now);
                let handle = call_addr(c, PLAY_SOUND, &[bank, TEST_SOUND, 0, level, 64]);
                c.m.w32(TEST_HANDLE, handle);
            }
        } else if m.r8(TEST_PLAYING) == 1 {
            let handle = m.r32(TEST_HANDLE);
            call_addr(c, SND_STOP, &[handle]);
            c.m.w8(TEST_PLAYING, 0);
        }
    }
    c.m.w8(WAS_IN_AUDIO_MENU, in_audio_menu as u8);
}

pub fn abi_music_logic(c: &mut Cpu) {
    music_logic(c, c.r[4]);
}

/// `MPause_ControllerLogic` (MPAUSE.CPP, 0x8009f1ac): the vibration items disabled for a pad
/// without it, enabled for one with it.
pub fn controller_logic(c: &mut Cpu) {
    let port = (c.m.r8(PAUSE_PORT_INDEX) != 0) as u32;
    let no_vibration = call_addr(c, PAD_GET_STATE, &[port << 4]) == PAD_NO_VIBRATION;
    for off in [ITEM_SHOCK_MODE, ITEM_SHOCK_IMPACT] {
        let d = defs(&mut c.m);
        set_disabled(&mut c.m, d, off, no_vibration);
    }
}

pub fn abi_controller_logic(c: &mut Cpu) {
    controller_logic(c);
}

/// The pad's keys the menu takes: the button (`Input_Interface`'s), whether only its press
/// counts, the key.
const KEYS: [(u32, bool, u32); 7] = [
    (5, true, pm::KEY_ACCEPT),
    (1, true, pm::KEY_UP),
    (2, true, pm::KEY_DOWN),
    (3, false, pm::KEY_LEFT),
    (4, false, pm::KEY_RIGHT),
    (6, true, pm::KEY_START),
    (24, true, pm::KEY_BACK),
];

/// The line the highlight bar is on for item `k` of `menu`.
fn bar_line(m: &mut Mem, menu: PMenu, k: i32) -> i32 {
    pm::item_enabled_num(m, menu, k).wrapping_mul(LINE).wrapping_add(FIRST_LINE)
}

/// `MPause_Logic` (MPAUSE.CPP, 0x8009f290): a frame of the pause menu: the pad's key (left and
/// right repeating where the item wants) to the menu once the highlight bar has stopped (and
/// the bar sent moving to a new current item), whether to draw; then the command: go to a menu
/// (confirming first where asked), back (or carry on from the first menu), or the race's
/// (returned: carry on 1, restart 2, quit 4, forfeit 5; 0 while the menu stays up, after the
/// audio menu's sounds and the vibration items).
pub fn logic(c: &mut Cpu) -> u32 {
    let menu = current_menu(&mut c.m);
    let repeat = pm::debounce(c, menu);
    with_buffer(c, 16, |c, buf| {
        let (command, key) = (PMenuCommand(buf), buf + 8);
        c.m.w32(key, 0);
        command.set_next_menu(&mut c.m, PMenu(0));
        for (button, pressed, k) in KEYS {
            if call_addr(c, INPUT_INTERFACE, &[button, if pressed { 1 } else { repeat }]) != 0 {
                c.m.w32(key, k);
                break;
            }
        }
        let m = &mut c.m;
        command.set_type_(m, 0);
        if m.r16(MOVING_HIGHLIGHT) != 0 {
            m.w32(UPDATE, 1);
        } else if m.r32(key) != 0 {
            m.w32(UPDATE, 1);
            let menu = current_menu(m);
            let before = menu.current_item(m);
            call_virtual(c, menu.0, MENU_VPTR, MENU_PROCESS_INPUT, &[key, command.0]);
            let m = &mut c.m;
            let menu = current_menu(m);
            let (before, now) = (before as i16 as i32, m.r16(menu.0) as i16 as i32);
            if before != now {
                let from = bar_line(m, menu, before);
                let menu = current_menu(m);
                let to = bar_line(m, menu, now);
                m.w16(MOVING_HIGHLIGHT, from.wrapping_sub(to) as u16);
                m.w16(HIGHLIGHT_STEP, (to.wrapping_sub(from) / 10) as u16);
            }
        } else if m.r32(CHANGED_ENABLING) != 0 {
            m.w32(UPDATE, 1);
        } else if m.r32(UPDATE_NEXT_TIME) != 0 {
            m.w32(UPDATE, 1);
            m.w32(UPDATE_NEXT_TIME, 0);
        } else {
            m.w32(UPDATE, 0);
        }
        let m = &mut c.m;
        let kind = command.type_(m) as u32;
        if kind != 0 {
            if kind & COMMAND_CONFIRM != 0 {
                let menu = current_menu(m);
                let k = menu.current_item(m) as u32;
                let item = PMenuItem(m.r32(menu.0.wrapping_add(8).wrapping_add(k.wrapping_mul(4))));
                let d = defs(m);
                let text = item.text_description(m);
                d.item_confirm_title().base().set_text_description(m, text);
                let confirm = d.0.wrapping_add(0x304);
                PMenu(confirm).set_current_item(m, 0);
                m.w32(d.0.wrapping_add(0x300), kind & 0xFF);
                command.set_type_(m, pm::COMMAND_GO_TO as i32);
                command.set_next_menu(m, PMenu(confirm));
            }
            let next = match command.type_(m) as u32 {
                pm::COMMAND_GO_TO => {
                    let depth = m.r32(BACK_DEPTH);
                    let menu = m.r32(CURRENT_MENU);
                    m.w32(BACK_LIST.wrapping_add(depth.wrapping_mul(4)), menu);
                    (depth.wrapping_add(1), command.next_menu(m).0)
                }
                pm::COMMAND_BACK => {
                    let depth = m.r32(BACK_DEPTH);
                    if depth as i32 <= 0 {
                        return pm::COMMAND_CONTINUE;
                    }
                    let depth = depth - 1;
                    (depth, m.r32(BACK_LIST.wrapping_add(depth.wrapping_mul(4))))
                }
                other => return other,
            };
            m.w32(BACK_DEPTH, next.0);
            m.w32(CURRENT_MENU, next.1);
            call_virtual(c, next.1, MENU_VPTR, MENU_INITIALIZE, &[]);
        }
        let m = &mut c.m;
        let audio = defs(m).0.wrapping_add(MENU_AUDIO_SETTINGS);
        let in_audio = (m.r32(CURRENT_MENU) == audio) as u32;
        music_logic(c, in_audio);
        controller_logic(c);
        0
    })
}

pub fn abi_logic(c: &mut Cpu) {
    c.r[2] = logic(c);
}

/// `MPause_Render` (MPAUSE.CPP, 0x8009f5ac): the highlight bar moved on its way (stopped at the
/// item), the menu drawn (the current item highlighted once the bar is there), the bar and the
/// box round the menu.
pub fn render(c: &mut Cpu) {
    let m = &mut c.m;
    m.w32(CHANGED_ENABLING, 0);
    let bar = m.r16(MOVING_HIGHLIGHT) as i16;
    if bar != 0 {
        let moved = (bar as u16).wrapping_add(m.r16(HIGHLIGHT_STEP)) as i16;
        m.w16(MOVING_HIGHLIGHT, moved as u16);
        // (past the item: there)
        if (bar > 0 && moved < 0) || (bar < 0 && moved > 0) {
            m.w16(MOVING_HIGHLIGHT, 0);
        }
    }
    let moving = m.r16(MOVING_HIGHLIGHT) != 0;
    current_menu(m).set_highlight(m, !moving as u32);
    let menu = current_menu(m);
    call_virtual(c, menu.0, MENU_VPTR, MENU_DRAW, &[]);
    let m = &mut c.m;
    let menu = current_menu(m);
    let items = pm::num_enabled_items(m, menu);
    let menu = current_menu(m);
    let k = menu.current_item(m);
    let line = pm::item_enabled_num(m, menu, k).wrapping_mul(LINE);
    let y = line.wrapping_add(m.r16(MOVING_HIGHLIGHT) as i16 as i32).wrapping_add(FIRST_LINE);
    call_addr(c, FBUILD_F4, &[1, 80, y as u32, 160, LINE as u32, 0, 0, 0]);
    let height = (items + 1).wrapping_mul(LINE).wrapping_add(20);
    call_addr(c, RENDER_PAUSE_BOX, &[80, 100, 160, height as u32]);
}

pub fn abi_render(c: &mut Cpu) {
    render(c);
}

/// `MPause_InitMPause` (MPAUSE.CPP, 0x8009f754): the language's in-game texts loaded, the menus
/// made.
pub fn init(c: &mut Cpu) {
    let language = c.m.r32(LANGUAGE);
    call_addr(c, LOAD_IN_GAME, &[language]);
    let at = call_addr(c, NEW, &[DEFS_SIZE]);
    let d = defs_ctor(c, PauseMenuDefs(at));
    c.m.w32(DEFS, d);
}

pub fn abi_init(c: &mut Cpu) {
    init(c);
}

/// `MPause_StartPauseMenu` (MPAUSE.CPP, 0x8009f78c): the pause menu up (the stack emptied);
/// Restart only where the race can restart, Forfeit in pink slips instead of Quit, the music's
/// level on; the commands confirmed in a one player race (the replay's first modes).
pub fn start_pause_menu(c: &mut Cpu) {
    let m = &mut c.m;
    let menu = defs(m).menu_pause();
    m.w32(CURRENT_MENU, menu.0);
    call_virtual(c, menu.0, MENU_VPTR, MENU_INITIALIZE, &[]);
    let m = &mut c.m;
    m.w32(BACK_DEPTH, 0);
    let race = GAME_SETUP.race_type(m);
    let d = defs(m);
    set_disabled(m, d, ITEM_RESTART, race == RACE_TYPE_PINK_SLIPS || race == RACE_TYPE_NO_RESTART);
    let pink_slips = GAME_SETUP.race_type(m) == RACE_TYPE_PINK_SLIPS;
    let order = if pink_slips { [(ITEM_FORFEIT_RACE, false), (ITEM_QUIT_RACE, true)] } else { [(ITEM_FORFEIT_RACE, true), (ITEM_QUIT_RACE, false)] };
    for (off, disabled) in order {
        let d = defs(m);
        set_disabled(m, d, off, disabled);
    }
    let d = defs(m);
    set_disabled(m, d, ITEM_MUSIC_VOLUME, false);
    let confirm = (m.r32(REPLAY_MODE) as i32) < 2;
    let d = defs(m);
    for off in [ITEM_QUIT_RACE, ITEM_RESTART, ITEM_FORFEIT_RACE] {
        let at = d.0.wrapping_add(off + 12);
        let command = m.r32(at);
        m.w32(at, if confirm { command | COMMAND_CONFIRM } else { command & !COMMAND_CONFIRM });
    }
}

pub fn abi_start_pause_menu(c: &mut Cpu) {
    start_pause_menu(c);
}

/// `MPause_EndPauseMenu` (MPAUSE.CPP, 0x8009f9bc): the pausing player's controller set up again
/// with its configuration.
pub fn end_pause_menu(c: &mut Cpu) {
    let port = c.m.r8(PAUSE_PORT_INDEX) as u32;
    let config = c.m.r32(CONTROLLER_CONFIG.wrapping_add(4 * port));
    call_addr(c, RESET_CONTROLLER, &[port, config]);
}

pub fn abi_end_pause_menu(c: &mut Cpu) {
    end_pause_menu(c);
}

/// `MPause_KillMPause` (MPAUSE.CPP, 0x8009f9f8): the drawing finished, the menus freed, the
/// texts unloaded.
pub fn kill(c: &mut Cpu) {
    call_addr(c, DRAW_SYNC, &[0]);
    let d = c.m.r32(DEFS);
    if d != 0 {
        defs_dtor(c, d, 3);
    }
    call_addr(c, UNLOAD_WORDS, &[]);
}

pub fn abi_kill(c: &mut Cpu) {
    kill(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::pausemenu::tests::shuffle_drawing;
    use crate::test_util::{PAD_STATE, Rng, against_original};

    /// The race RAM's menus (made at the race's start), and room for another set.
    const D: u32 = 0x8015_29F8;
    const NEW_DEFS: u32 = 0x8014_3000;
    /// Its menus: pause, options, audio, controller, are you sure.
    const MENUS: [u32; 5] = [0x60, 0xE8, 0x1C8, 0x278, 0x304];
    /// The pad's buttons by tick (`Input_gInterfaceResults`, 32), the tick (`simGlobal` +8).
    const INPUT_RESULTS: u32 = 0x8011_51CC;
    const SIM_TICK: u32 = 0x8011_F36C;
    /// `Draw_gPauseMenuRect`'s neighbours: the view drawn, the flip.
    const RACE_TYPE: u32 = 0x8011_44A4;

    /// The race's RAM, its menus in any state the game can be in: any one up (on an item it
    /// can be on), up to three gone back to, the highlight bar still or moving, the items the
    /// game enables and disables either way, the commands confirmed or not; this tick's and
    /// the last's buttons; the audio mode, levels and test sound's state; a pad with or without
    /// vibration, either port; the race type, the replay's mode; the drawing's state.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        shuffle_drawing(c, rng);
        let m = &mut c.m;
        let menu = D + MENUS[pick(rng, 5) as usize];
        m.w32(CURRENT_MENU, menu);
        let n = m.r32(menu + 0x4C);
        m.w32(menu, 1 + pick(rng, n));
        m.w32(menu + 4, pick(rng, 2));
        m.w32(BACK_DEPTH, pick(rng, 4));
        for k in 0..6 {
            m.w32(BACK_LIST + 4 * k, D + MENUS[pick(rng, 5) as usize]);
        }
        // (still, or moving: a step from overshooting it now and then)
        let bar = [0, 0, -3, 3, -12, 12, pick(rng, 81) as i32 - 40][pick(rng, 7) as usize];
        m.w16(MOVING_HIGHLIGHT, bar as u16);
        m.w16(HIGHLIGHT_STEP, (pick(rng, 21) as i32 - 10) as u16);
        m.w32(CHANGED_ENABLING, pick(rng, 2));
        m.w32(UPDATE, pick(rng, 2));
        for off in [ITEM_RESTART, ITEM_QUIT_RACE, ITEM_FORFEIT_RACE, ITEM_MUSIC_VOLUME, ITEM_SHOCK_MODE, ITEM_SHOCK_IMPACT] {
            m.w32(D + off, (pick(rng, 3) == 0) as u32);
        }
        for (off, command) in [(ITEM_RESTART, 2), (ITEM_QUIT_RACE, 4), (ITEM_FORFEIT_RACE, 5)] {
            m.w32(D + off + 12, command | if pick(rng, 2) == 0 { COMMAND_CONFIRM } else { 0 });
        }
        m.w32(D + 0x300, [2, 4, 5, 7][pick(rng, 4) as usize]);
        let tick = m.r32(SIM_TICK);
        for t in [tick, tick.wrapping_sub(1)] {
            let mut bits = 0;
            for b in [5, 1, 2, 3, 4, 6, 24] {
                if pick(rng, 5) == 0 {
                    bits |= 1 << b;
                }
            }
            m.w32(INPUT_RESULTS + 4 * (t & 31), bits);
        }
        m.w32(AUDIO_MODE, pick(rng, 4));
        for level in [MUSIC_LEVEL, SFX_LEVEL, NARRATION_LEVEL, ENGINE_LEVEL, AMBIENT_LEVEL] {
            m.w32(level, pick(rng, 128));
        }
        m.w8(WAS_IN_AUDIO_MENU, pick(rng, 2) as u8);
        m.w8(TEST_WANTED, pick(rng, 2) as u8);
        m.w8(TEST_PLAYING, pick(rng, 3) as u8);
        let now = m.r32(GAME_TICKS);
        m.w32(TEST_STARTED, now.wrapping_sub([0, 192, 193, 400][pick(rng, 4) as usize]));
        m.w32(PAD_STATE, [0, PAD_NO_VIBRATION][pick(rng, 2) as usize]);
        m.w8(PAUSE_PORT_INDEX, pick(rng, 2) as u8);
        m.w32(RACE_TYPE, pick(rng, 8));
        m.w32(REPLAY_MODE, pick(rng, 4));
        m.w32(crate::globals::FLIP, pick(rng, 2));
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tPauseMenuDefs::tPauseMenuDefs", 0x8009_e998, nfs4rc::f_8009e998, |_, _| vec![NEW_DEFS]),
            ("tPauseMenuDefs::~tPauseMenuDefs", 0x8009_ecf8, nfs4rc::f_8009ecf8, |_, _| vec![D, 2]),
            ("MPause_MusicLogic", 0x8009_eeac, nfs4rc::f_8009eeac, |_, r| vec![[0, 1, 0x100, 0x101][pick(r, 4) as usize]]),
            ("MPause_ControllerLogic", 0x8009_f1ac, nfs4rc::f_8009f1ac, |_, _| vec![]),
            ("MPause_Logic", 0x8009_f290, nfs4rc::f_8009f290, |_, _| vec![]),
            ("MPause_Render", 0x8009_f5ac, nfs4rc::f_8009f5ac, |_, _| vec![]),
            ("MPause_StartPauseMenu", 0x8009_f78c, nfs4rc::f_8009f78c, |_, _| vec![]),
            ("MPause_EndPauseMenu", 0x8009_f9bc, nfs4rc::f_8009f9bc, |_, _| vec![]),
            ("MPause_KillMPause", 0x8009_f9f8, nfs4rc::f_8009f9f8, |_, _| vec![]),
            ("RPause_CopyBackToFrontBuffer", 0x800d_dad0, nfs4rc::f_800ddad0, |_, _| vec![]),
            ("RPause_StartPauseMenu", 0x800d_db90, nfs4rc::f_800ddb90, |_, _| vec![]),
            ("RPause_StopPauseMenu", 0x800d_dbb0, nfs4rc::f_800ddbb0, |_, _| vec![]),
        ]
    }

    #[test]
    fn mpause_fuzz() {
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x9B0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original(name, 300, 0x9B1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }
}
