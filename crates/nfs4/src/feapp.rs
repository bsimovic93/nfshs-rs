//! FEAPP.CPP: the front-end application (`tFEApplication`, `gFEApp`). It holds:
//! - its four dialogs (a message, the help, a timed message, one taking no input);
//! - each player's menu and screen, the ones being gone to, and the parent menu a sub-menu is
//!   drawn over;
//! - each player's stack of menus to go back through.
//!
//! It also chooses the music each menu plays and draws the help's icons at the bottom. A frame
//! is drawn one half per player: the menus' and screens' transitions, then their drawing, and
//! the dialogs. The main loop runs the front end until a race starts or the game quits.
//!
//! Written as FEMENU.CPP is (see there): typed views, calls through `crate::call`. The player
//! being run or drawn (`fPlayer`) is read again after each call, as the original does: menus'
//! code reads it, and may change it.

use crate::call::{call, call_addr, call_virtual, with_buffer};
use crate::femenu::{
    COMMAND_BACK, COMMAND_GO_TO, COMMAND_GO_TO_RESET, COMMAND_GO_TO_WITH_SECOND, COMMAND_QUIT, COMMAND_RACE, COMMAND_RACE_TOO, COMMAND_RACE_WHEN_BOTH_READY,
    KEY_DOWN, KEY_HELP, KEY_NONE, KEY_UP, PLAY_FE_SFX,
};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, GET_DRAWENV, SET_DRAW_AREA, SPRINTF, STRLEN, TEXT_PIXELS, TEXT_WORD, add_prim};
use crate::fetextrender;
use crate::globals::{FE_APP, FLIP, FRONT_END, GAME_TICKS, PAD_INFO, PLAYER1_VIEW, SCREEN_HEIGHT, SHAPES, SONGS_SCREEN};
use crate::types::{CarInfo, DRAWENV, DialogBase, DialogMessageString, FEApplication, Menu, MenuCommand, MenuItem, RECT, Screen};
use ps1rt::{Cpu, Mem};

/// A menu's vtable pointer and its virtual functions (their vtable entries' offsets).
const MENU_VPTR: u32 = 104;
const MENU_INITIALIZE: u32 = 16;
const MENU_PROCESS_INPUT: u32 = 24;
const MENU_DEBOUNCE_KEYS: u32 = 32;
const MENU_TRANSITION_OFF: u32 = 40;
const MENU_TRANSITION_ON: u32 = 48;
const MENU_TRANSITION_IS_FINISHED: u32 = 56;
const MENU_IS_SUB_MENU: u32 = 64;
const MENU_DRAW: u32 = 72;
const MENU_UPDATE_TRANSITION: u32 = 80;
/// A screen's (and so a dialog's): `Initialize`, `Cleanup`, `TransitionIsFinished`,
/// `ProcessInput`.
const SCREEN_VPTR: u32 = 96;
const SCREEN_INITIALIZE: u32 = 48;
const SCREEN_CLEANUP: u32 = 56;
const SCREEN_TRANSITION_IS_FINISHED: u32 = 64;
const SCREEN_PROCESS_INPUT: u32 = 72;

/// `tScreen`'s functions (ported in FESCREEN, called by address): the constructor and
/// destructor, `UpdateTransition`, `Draw(background)`, `TransitionOff` and `TransitionOn` (with
/// the transition type, always 2, and no menu).
const SCREEN_CTOR: u32 = 0x8002_6040;
const SCREEN_DTOR: u32 = 0x8002_6098;
const SCREEN_UPDATE_TRANSITION: u32 = 0x8002_6868;
const SCREEN_DRAW: u32 = 0x8002_620C;
const SCREEN_TRANSITION_OFF: u32 = 0x8002_6824;
const SCREEN_TRANSITION_ON: u32 = 0x8002_6844;
const TRANSITION: u32 = 2;
/// `operator delete`.
const DELETE: fn(&mut Cpu) = nfs4rc::f_800a4910;

/// The dialogs' vtables: the base's, a message's, the help's, a message timing out, one taking
/// no input.
const DIALOG_VT: u32 = 0x8001_05A8;
const MESSAGE_VT: u32 = 0x8001_04E8;
const HELP_VT: u32 = 0x8001_0548;
const TIMED_MESSAGE_VT: u32 = 0x8001_0090;
const NO_INPUT_VT: u32 = 0x8001_0428;
/// The dialogs' functions: the top-most one shown; `Display` and `Hide` (hooked by the host:
/// called by address).
const GET_TOP_MOST_DIALOG: u32 = 0x8001_8970;
const DISPLAY: u32 = 0x8001_8980;
const HIDE: u32 = 0x8001_8A44;

/// The music's volume (0x8007bc44), from the front end's setting (`frontEnd + 833`); a song
/// (`AudioMus_PlaySong`, called by address: the host logs it) for each of the menus' music flags
/// (the menus', the garage's, the showcase's, victory's), and the song stopped.
const MUSIC_VOLUME: fn(&mut Cpu) = nfs4rc::f_8007bc44;
const VOLUME: u32 = FRONT_END + 833;
const MUSIC_FLAGS: u32 = 0xF000;
const SONGS: [(u32, u32); 4] = [(0x1000, 0x8001_0044), (0x2000, 0x8001_004C), (0x4000, 0x8001_0054), (0x8000, 0x8001_0060)];
const PLAY_SONG: u32 = 0x8007_B9C4;
const STOP_SONG: fn(&mut Cpu) = nfs4rc::f_8007b798;
/// The clock's start and end; whether frames wait for the vertical blank (`Draw_gDoVSync`).
const CLOCK_START: fn(&mut Cpu) = nfs4rc::f_8008c4d4;
const CLOCK_END: fn(&mut Cpu) = nfs4rc::f_8008c530;
const DO_VSYNC: u32 = 0x8013_EA90;

/// The help's icons: their colour, the help and options texts, their shapes (the PlayStation
/// pad's help and options buttons, the NeGcon's) and the shapes' flags.
const ICON_COLOR: u32 = 0x0078_6E14;
const HELP_TEXT: u32 = 253;
const OPTIONS_TEXT: u32 = 254;
const PAD_HELP: u32 = 53;
const NEGCON_HELP: u32 = 54;
const PAD_OPTIONS: u32 = 55;
const NEGCON_OPTIONS: u32 = 56;
const ICON_FLAGS: u32 = 24;
/// The pads (`PAD_INFO`, 32 bytes each): +4 the status (0 plugged in), +5 the kind.
const PAD_STATUS: u32 = 4;
const PAD_KIND: u32 = 5;
const PAD_SIZE: u32 = 32;
const NEGCON: u32 = 0x23;

/// The front end's shapes (`SHAPES`, 32 bytes each, a CLUT id at +4): the help's are the first
/// 59; `Texture_MenuReleaseClutId`.
const HELP_SHAPES: u32 = 59;
const SHAPE_SIZE: u32 = 32;
const SHAPE_CLUT: u32 = 4;
const RELEASE_CLUT: u32 = 0x800e_05b0;

/// A frame's drawing: `FeAudio_systemtask`, `Draw_StartFrameRender`, `Draw_StartRenderingView`,
/// `Draw_StopRenderingView`, the frame shown.
const SYSTEM_TASK: fn(&mut Cpu) = crate::feaudio::systemtask_entry;
const START_FRAME: fn(&mut Cpu) = nfs4rc::f_800bf044;
const START_VIEW: fn(&mut Cpu) = nfs4rc::f_800bedac;
const STOP_VIEW: fn(&mut Cpu) = nfs4rc::f_800bee9c;
const SHOW_FRAME: fn(&mut Cpu) = nfs4rc::f_800bf0f0;
/// `Redraw` itself: hooked (the host autosaves between front-end frames), so called by address.
const REDRAW: u32 = 0x8001_3894;
/// The front end's menus (`*0x80051ee0`, FEMENUDEFS's): the main menu at +176; two of its
/// items, disabled while a pad is out: one needing the first pad (+0), one needing both (+44).
const MENU_DEFS: u32 = SONGS_SCREEN;
const MAIN_MENU: u32 = 176;
const ITEM_NEEDS_FIRST_PAD: u32 = 0;
const ITEM_NEEDS_BOTH_PADS: u32 = 44;
const ITEM_DISABLED: u32 = 1;
/// The main menu's `ProcessInput` (`tMenuNFS4`'s): moved off an item disabled.
const MAIN_MENU_PROCESS_INPUT: fn(&mut Cpu, u32, u32, u32, u32) = crate::femenuextended::menu_nfs4_process_input;

/// The front end's sounds: accept, back.
const SOUND_ACCEPT: u32 = 0;
const SOUND_BACK: u32 = 1;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

/// The player being run or drawn.
fn player(m: &mut Mem, this: FEApplication) -> u32 {
    this.player(m) as u8 as u32
}

/// That player's menu, screen and parent menu.
fn current_menu(m: &mut Mem, this: FEApplication) -> Menu {
    let p = player(m, this);
    this.current_menu(m, p)
}

fn current_screen(m: &mut Mem, this: FEApplication) -> Screen {
    let p = player(m, this);
    this.current_screen(m, p)
}

fn current_parent(m: &mut Mem, this: FEApplication) -> Menu {
    let p = player(m, this);
    this.parent_menu(m, p)
}

/// A menu's virtual function at `slot` (none takes arguments); its result.
fn menu_fn(c: &mut Cpu, menu: Menu, slot: u32) -> u32 {
    call_virtual(c, menu.0, MENU_VPTR, slot, &[])
}

/// A screen's.
fn screen_fn(c: &mut Cpu, screen: Screen, slot: u32) -> u32 {
    call_virtual(c, screen.0, SCREEN_VPTR, slot, &[])
}

/// A dialog as `tDialogBase`'s constructor leaves it (its `tScreen` made first): for either
/// player, 288 wide at most, hidden, no timeout.
fn dialog_base(m: &mut Mem, d: DialogBase) {
    d.set_vtable(m, DIALOG_VT);
    d.set_currently_on(m, 0);
    d.set_reservedheight(m, 0);
    d.set_max_h(m, 0);
    d.set_offset_y(m, 0);
    d.set_offset_x(m, 0);
    d.set_height(m, 0);
    d.set_width(m, 0);
    d.set_top(m, 0);
    d.set_left(m, 0);
    d.set_max_w(m, 288);
    d.set_specific_player(m, -1);
    d.set_default(m, 0);
    d.set_time_out_ticks(m, 0);
}

/// A message dialog (`tDialogMessageString`): not centred, its text unfaded.
fn message(m: &mut Mem, d: DialogMessageString) {
    dialog_base(m, *d);
    d.set_vtable(m, MESSAGE_VT);
    d.set_centerit(m, 0);
    d.set_fully_open(m, 0);
    d.set_time_out_ticks(m, 0);
    d.set_fade_text(m, 128);
}

/// `tFEApplication::tFEApplication` (FEAPP.CPP, 0x800130f8): its four dialogs made (a message,
/// the help timing out after 1400 ticks, a message timing out after 1152, one taking no input),
/// no names got or needed, no speech to play.
pub fn fe_application(c: &mut Cpu, this: FEApplication) -> FEApplication {
    let screen = |c: &mut Cpu, d: u32| call_addr(c, SCREEN_CTOR, &[d]);
    // (each dialog's fields set before the next one's screen is made, as the original does)
    let d = this.message_popup();
    screen(c, d.0);
    message(&mut c.m, d);
    let d = this.help_popup();
    screen(c, d.0);
    let m = &mut c.m;
    dialog_base(m, *d);
    d.set_vtable(m, HELP_VT);
    d.set_variant(m, -1);
    d.set_time_out_ticks(m, 1400);
    let d = this.mem_card_dialog();
    screen(c, d.0);
    let m = &mut c.m;
    message(m, *d);
    d.set_vtable(m, TIMED_MESSAGE_VT);
    d.set_time_out_ticks(m, 1152);
    let d = this.no_input_mem_card_dialog();
    screen(c, d.0);
    let m = &mut c.m;
    message(m, *d);
    d.set_vtable(m, NO_INPUT_VT);
    for p in 0..2 {
        this.set_got_name(m, p, 0);
        this.set_need_name(m, p, 0);
        this.set_speech_to_play(m, p, -1);
    }
    this
}

pub fn abi_fe_application(c: &mut Cpu) {
    c.r[2] = fe_application(c, FEApplication(c.r[4])).0;
}

/// `tFEApplication::~tFEApplication` (FEAPP.CPP, 0x8001331c): its dialogs destroyed (the last
/// first), it deleted when `flags` asks.
pub fn fe_application_dtor(c: &mut Cpu, this: FEApplication, flags: u32) {
    for d in [this.no_input_mem_card_dialog().0, this.mem_card_dialog().0, this.help_popup().0, this.message_popup().0] {
        call_addr(c, SCREEN_DTOR, &[d, 2]);
    }
    if flags & 1 != 0 {
        call(c, DELETE, &[this.0]);
    }
}

pub fn abi_fe_application_dtor(c: &mut Cpu) {
    fe_application_dtor(c, FEApplication(c.r[4]), c.r[5]);
}

/// The music's volume, from the front end's setting (`* 35 / 64`).
fn music_volume(m: &mut Mem) -> u32 {
    let v = m.r8(VOLUME) as u32;
    (v * 35) >> 6
}

/// `tFEApplication::PerformMenuInitialization` (FEAPP.CPP, 0x8001338c): no music, the music's
/// volume, no dialogs, the clock started, frames waiting for the vertical blank, the text's
/// blending off.
pub fn perform_menu_initialization(c: &mut Cpu, this: FEApplication) {
    this.set_current_music(&mut c.m, 0);
    let volume = music_volume(&mut c.m);
    call(c, MUSIC_VOLUME, &[volume]);
    crate::fedialog::abi_initialize_class(c);
    call(c, CLOCK_START, &[]);
    c.m.w32(DO_VSYNC, 1);
    fetextrender::set_abr(c, 0, 0);
}

pub fn abi_perform_menu_initialization(c: &mut Cpu) {
    perform_menu_initialization(c, FEApplication(c.r[4]));
}

/// `tFEApplication::PerformMenuDestruction` (FEAPP.CPP, 0x800133f4): the clock ended, each
/// player's screen cleaned up and gone, the music stopped, frames not waiting, the text's
/// blending off.
pub fn perform_menu_destruction(c: &mut Cpu, this: FEApplication) {
    call(c, CLOCK_END, &[]);
    for p in 0..2 {
        let screen = this.current_screen(&mut c.m, p);
        if screen.0 != 0 {
            screen_fn(c, screen, SCREEN_CLEANUP);
        }
        this.set_current_screen(&mut c.m, p, Screen(0));
    }
    call(c, STOP_SONG, &[1000]);
    c.m.w32(DO_VSYNC, 0);
    fetextrender::set_abr(c, 0, 0);
}

pub fn abi_perform_menu_destruction(c: &mut Cpu) {
    perform_menu_destruction(c, FEApplication(c.r[4]));
}

/// Whether a pad of either player is plugged in and is not a NeGcon (`negcon` false), or is a
/// NeGcon (`negcon` true: plugged in or not).
fn pads(m: &mut Mem, negcon: bool) -> bool {
    (0..2).any(|p| {
        let pad = PAD_INFO + PAD_SIZE * p;
        let is_negcon = m.r8(pad + PAD_KIND) as u32 == NEGCON;
        if negcon { is_negcon } else { m.r8(pad + PAD_STATUS) == 0 && !is_negcon }
    })
}

/// `tFEApplication::DrawHelpIcons` (FEAPP.CPP, 0x8001349c): at the bottom left, the help
/// button (the PlayStation pad's and the NeGcon's, for the pads there are) and "help", then the
/// options button and "options" when either player's menu has options. For a menu with its
/// help down the side, "help" is written up the side a letter at a time, its buttons above it.
pub fn draw_help_icons(c: &mut Cpu, this: FEApplication) {
    // the shapes' options (only their colour, at +4, set), a letter, a text
    with_buffer(c, 96, |c, at| {
        let (options, letter, text) = (at, at + 24, at + 32);
        c.m.w32(options + 4, ICON_COLOR);
        let shape = |c: &mut Cpu, k: u32, x: u32, y: u32| call(c, DRAW_SHAPE_EXTENDED, &[k, ICON_FLAGS, x, y, 0, 0, options]);
        let text_at = |c: &mut Cpu, text: u32, x: u32, y: u32| fetextrender::full_text_rgb(c, text, sx(x), sx(y), ICON_COLOR, 0, 0);
        let mut y = c.m.r32(SCREEN_HEIGHT).wrapping_sub(25);
        let menu = this.current_menu(&mut c.m, 0);
        if menu.0 != 0 && menu.vert_help(&mut c.m) != 0 {
            // up the side: the last letter lowest
            c.m.w8(letter + 1, 0);
            let word = call(c, TEXT_WORD, &[HELP_TEXT]);
            call(c, SPRINTF, &[text, word]);
            let n = call(c, STRLEN, &[text]);
            for k in (0..n).rev() {
                let b = c.m.r8(text + k);
                c.m.w8(letter, b);
                text_at(c, letter, 14, y);
                y = y.wrapping_sub(8);
            }
            y = y.wrapping_add(8);
            if pads(&mut c.m, false) {
                y = y.wrapping_sub(8);
                shape(c, PAD_HELP, 14, y);
            }
            if pads(&mut c.m, true) {
                shape(c, NEGCON_HELP, 13, y.wrapping_sub(9));
            }
            return;
        }
        let mut x = 14u32;
        let buttons = |c: &mut Cpu, x: &mut u32, (pad, negcon): (u32, u32)| {
            if pads(&mut c.m, false) {
                shape(c, pad, *x, y);
                *x += 14;
            }
            if pads(&mut c.m, true) {
                shape(c, negcon, *x, y);
                *x += 15;
            }
        };
        buttons(c, &mut x, (PAD_HELP, NEGCON_HELP));
        let word = call(c, TEXT_WORD, &[HELP_TEXT]);
        text_at(c, word, x, y);
        // past the text: its width in pixels less its length, and 5
        let word = call(c, TEXT_WORD, &[HELP_TEXT]);
        let w = call(c, TEXT_PIXELS, &[word]);
        let word = call(c, TEXT_WORD, &[HELP_TEXT]);
        let len = call(c, STRLEN, &[word]);
        x = x.wrapping_add(5).wrapping_add(w.wrapping_sub(len));
        let m = &mut c.m;
        let (first, second) = (this.current_menu(m, 0), this.current_menu(m, 1));
        let options = first.options_menu(m).0 != 0 || (second.0 != 0 && second.options_menu(m).0 != 0);
        if !options {
            return;
        }
        buttons(c, &mut x, (PAD_OPTIONS, NEGCON_OPTIONS));
        let word = call(c, TEXT_WORD, &[OPTIONS_TEXT]);
        text_at(c, word, x, y);
    })
}

pub fn abi_draw_help_icons(c: &mut Cpu) {
    draw_help_icons(c, FEApplication(c.r[4]));
}

/// The draw area set to the screen's width from `top` down, `h` high (in the draw environment
/// `env`), as a primitive in the ordering table.
fn draw_area(c: &mut Cpu, env: DRAWENV, top: u32, h: u32) {
    with_buffer(c, 8, |c, at| {
        let (rect, m) = (RECT(at), &mut c.m);
        let y = (env.clip().y(m) as u16 as u32).wrapping_add(top);
        rect.set_x(m, 0);
        rect.set_y(m, y as i16);
        rect.set_w(m, 512);
        rect.set_h(m, h as i16);
        let prim = add_prim(m);
        call_addr(c, SET_DRAW_AREA, &[prim, at]);
    })
}

/// A pad out: the main menu's item needing it disabled (else enabled), and the main menu moved
/// off it (sent `key`) when it is the first player's current item.
fn pad_out_item(c: &mut Cpu, this: FEApplication, out: bool, item: u32, key: u32) {
    let item = MenuItem(c.m.r32(MENU_DEFS).wrapping_add(item));
    let m = &mut c.m;
    if !out {
        item.update_flags(m, |f| f & !ITEM_DISABLED);
        return;
    }
    item.update_flags(m, |f| f | ITEM_DISABLED);
    let menu = this.current_menu(m, 0);
    let k = menu.current_item(m) as u32;
    if menu.item_list(m, k) == item {
        let main = m.r32(MENU_DEFS).wrapping_add(MAIN_MENU);
        // the key and a command to take what the menu does with it
        with_buffer(c, 16, |c, at| {
            c.m.w32(at + 8, key);
            MAIN_MENU_PROCESS_INPUT(c, main, 0, at + 8, at);
        });
    }
}

/// `tFEApplication::Redraw` (FEAPP.CPP, 0x80013894): a frame. The dialogs, the help's icons,
/// the main menu's items needing a pad that is out; then each player's half, the second's first:
/// its menus' and screen's transitions; then, front to back (the ordering table draws what is
/// added later under what came before): a sub-menu, the screen's foreground, the menu (unless
/// waiting for the other player, but going on) and the parent menu, the screen's background; the
/// frame shown.
pub fn redraw(c: &mut Cpu, this: FEApplication) {
    let was = this.player(&mut c.m);
    call(c, SYSTEM_TASK, &[0]);
    call(c, START_FRAME, &[]);
    let view = c.m.r32(PLAYER1_VIEW);
    call(c, START_VIEW, &[view]);
    let (view, flip) = (c.m.r32(PLAYER1_VIEW), c.m.r32(FLIP));
    let env = DRAWENV(call(c, GET_DRAWENV, &[view, flip]));
    let m = &mut c.m;
    let half = if this.current_menu(m, 1).0 != 0 { (m.r32(SCREEN_HEIGHT) as i32 / 2) as u32 } else { m.r16(SCREEN_HEIGHT) as u32 };
    crate::fedialog::abi_draw_all_dialogs(c);
    draw_help_icons(c, this);
    let (first, second) = (c.m.r8(PAD_INFO + PAD_STATUS) != 0, c.m.r8(PAD_INFO + PAD_SIZE + PAD_STATUS) != 0);
    pad_out_item(c, this, first || second, ITEM_NEEDS_BOTH_PADS, KEY_UP);
    let first = c.m.r8(PAD_INFO + PAD_STATUS) != 0;
    pad_out_item(c, this, first, ITEM_NEEDS_FIRST_PAD, KEY_DOWN);
    let h = c.m.r16(SCREEN_HEIGHT) as u32;
    draw_area(c, env, 0, h);
    for p in [1u32, 0] {
        let m = &mut c.m;
        this.set_player(m, p as i8);
        this.set_y_offset(m, (p * half) as i16);
        // the transitions
        let menu = this.current_menu(m, p);
        if menu.0 != 0 {
            menu_fn(c, menu, MENU_UPDATE_TRANSITION);
        }
        let parent = current_parent(&mut c.m, this);
        if parent.0 != 0 {
            menu_fn(c, parent, MENU_UPDATE_TRANSITION);
        }
        let screen = current_screen(&mut c.m, this);
        if screen.0 != 0 {
            call_addr(c, SCREEN_UPDATE_TRANSITION, &[screen.0]);
        }
        // the drawing, front to back
        let menu = current_menu(&mut c.m, this);
        if menu.0 != 0 && menu_fn(c, menu, MENU_IS_SUB_MENU) != 0 {
            let menu = current_menu(&mut c.m, this);
            menu_fn(c, menu, MENU_DRAW);
        }
        let screen = current_screen(&mut c.m, this);
        if screen.0 != 0 {
            call_addr(c, SCREEN_DRAW, &[screen.0, 0]);
        }
        let menu = current_menu(&mut c.m, this);
        if menu.0 != 0 {
            let p = player(&mut c.m, this);
            let draw = this.waiting_for_other_player(&mut c.m, p) == 0
                || (menu_fn(c, menu, MENU_TRANSITION_IS_FINISHED) == 0 && {
                    let menu = current_menu(&mut c.m, this);
                    menu_fn(c, menu, MENU_IS_SUB_MENU) == 0
                });
            if draw {
                let menu = current_menu(&mut c.m, this);
                menu_fn(c, menu, MENU_DRAW);
            }
            let parent = current_parent(&mut c.m, this);
            if parent.0 != 0 {
                menu_fn(c, parent, MENU_DRAW);
            }
        }
        let screen = current_screen(&mut c.m, this);
        if screen.0 != 0 {
            call_addr(c, SCREEN_DRAW, &[screen.0, 1]);
        }
        let top = this.y_offset(&mut c.m) as u16 as u32;
        draw_area(c, env, top, half);
    }
    this.set_player(&mut c.m, was);
    let view = c.m.r32(PLAYER1_VIEW);
    call(c, STOP_VIEW, &[view]);
    call(c, SHOW_FRAME, &[]);
}

pub fn abi_redraw(c: &mut Cpu) {
    redraw(c, FEApplication(c.r[4]));
}

/// `tFEApplication::UpdateMusic` (FEAPP.CPP, 0x80013e34): the music's volume; the song the
/// menus' music flags ask for, else none.
pub fn update_music(c: &mut Cpu, this: FEApplication) {
    let volume = music_volume(&mut c.m);
    call(c, MUSIC_VOLUME, &[volume]);
    let flags = this.current_music(&mut c.m);
    match SONGS.iter().find(|&&(flag, _)| flags & flag != 0) {
        Some(&(_, song)) => call_addr(c, PLAY_SONG, &[song]),
        None => call(c, STOP_SONG, &[1000]),
    };
}

pub fn abi_update_music(c: &mut Cpu) {
    update_music(c, FEApplication(c.r[4]));
}

/// `tFEApplication::SetMenu` (FEAPP.CPP, 0x80013ef4): `player`'s next menu (the first player's
/// changing the music when its music flags do). The menu now goes off, unless the next one is a
/// sub-menu drawn over it. The menu's screen is next.
pub fn set_menu(c: &mut Cpu, this: FEApplication, player: u32, menu: Menu) {
    let p = sx(player);
    if menu == this.current_menu(&mut c.m, p) {
        return;
    }
    if p == 0 {
        let flags = menu.flags(&mut c.m) & MUSIC_FLAGS;
        if flags != this.current_music(&mut c.m) {
            this.set_current_music(&mut c.m, flags);
            update_music(c, this);
        }
    }
    let sub = menu_fn(c, menu, MENU_IS_SUB_MENU) == 1;
    let now = this.current_menu(&mut c.m, p);
    this.set_transition_to_menu(&mut c.m, p, menu);
    if !sub && now.0 != 0 && menu.0 != 0 {
        menu_fn(c, now, MENU_TRANSITION_OFF);
    }
    let screen = menu.screen(&mut c.m);
    set_screen(c, this, p, screen);
}

pub fn abi_set_menu(c: &mut Cpu) {
    set_menu(c, FEApplication(c.r[4]), c.r[5], Menu(c.r[6]));
}

/// `tFEApplication::SetScreen` (FEAPP.CPP, 0x80013ff8): `player`'s next screen; the screen now
/// going off.
pub fn set_screen(c: &mut Cpu, this: FEApplication, player: u32, screen: Screen) {
    let p = sx(player);
    let now = this.current_screen(&mut c.m, p);
    if screen == now {
        return;
    }
    this.set_transition_to_screen(&mut c.m, p, screen);
    if now.0 != 0 && screen.0 != 0 {
        call_addr(c, SCREEN_TRANSITION_OFF, &[now.0, TRANSITION, 0]);
    }
}

pub fn abi_set_screen(c: &mut Cpu) {
    set_screen(c, FEApplication(c.r[4]), c.r[5], Screen(c.r[6]));
}

/// `tFEApplication::DisplayHelp` (FEAPP.CPP, 0x80014048): the help shown on `page`.
pub fn display_help(c: &mut Cpu, this: FEApplication, page: u32) {
    let help = this.help_popup();
    help.set_variant(&mut c.m, page as i16);
    call_addr(c, DISPLAY, &[help.0]);
}

pub fn abi_display_help(c: &mut Cpu) {
    display_help(c, FEApplication(c.r[4]), c.r[5]);
}

/// `FreeHelpShapeCluts` (FEAPP.CPP, 0x8001406c): the help shapes' CLUTs freed.
pub fn free_help_shape_cluts(c: &mut Cpu) {
    for k in 0..HELP_SHAPES {
        let shape = c.m.r32(SHAPES).wrapping_add(k * SHAPE_SIZE);
        if c.m.r32(shape + SHAPE_CLUT) != 0 {
            let clut = c.m.r16(shape + SHAPE_CLUT) as i16 as i32 as u32;
            crate::call::call_jal(c, RELEASE_CLUT, &[clut]);
            let shape = c.m.r32(SHAPES).wrapping_add(k * SHAPE_SIZE);
            c.m.w32(shape + SHAPE_CLUT, 0);
        }
    }
}

pub fn abi_free_help_shape_cluts(c: &mut Cpu) {
    free_help_shape_cluts(c);
}

/// `tFEApplication::RunFrontEnd` (FEAPP.CPP, 0x8001585c): the main loop from the main menu.
pub fn run_front_end(c: &mut Cpu, this: FEApplication) -> u32 {
    let main = Menu(c.m.r32(MENU_DEFS).wrapping_add(MAIN_MENU));
    main_loop(c, this, main)
}

pub fn abi_run_front_end(c: &mut Cpu) {
    c.r[2] = run_front_end(c, FEApplication(c.r[4]));
}

/// The memory left (`largestunused`, kept at 0x800519a0). The race's start (`GameSetup_gData + 0x24`: 0 a race, 2 quit).
const LARGEST_UNUSED: fn(&mut Cpu) = nfs4rc::f_800e5e70;
const MEMORY_LEFT: u32 = 0x8005_19A0;
const GAME_START: u32 = 0x8011_44C8;
/// The players' keys: `FEInput_GetKeyFromPlayer`; a key repeats after 16 ticks.
const GET_KEY_FROM_PLAYER: u32 = 0x8002_3CA8;
const KEY_REPEAT: u32 = 16;
/// Whether two players are racing (`frontEnd + 3`); the race type (`+4`: 6 pink slips), whether
/// the player's car is from the garage (`+298`); the cars chosen (stock `+92`, garage `+291`,
/// pink slips `+293`, one byte for each player); the car manager and its cars (`GetStockCar`,
/// `GetGarageCar`, `GetPinkSlipsCar`).
const TWO_PLAYERS: u32 = FRONT_END + 3;
const RACE_TYPE: u32 = FRONT_END + 4;
const PINK_SLIPS: u32 = 6;
const GARAGE_CAR: u32 = FRONT_END + 298;
const STOCK_CARS: u32 = FRONT_END + 92;
const GARAGE_CARS: u32 = FRONT_END + 291;
const PINK_SLIPS_CARS: u32 = FRONT_END + 293;
const CAR_MANAGER: u32 = 0x8011_552C;
const GET_STOCK_CAR: u32 = 0x8001_6FD8;
const GET_GARAGE_CAR: u32 = 0x8001_7094;
const GET_PINK_SLIPS_CAR: u32 = 0x8001_71A0;
/// The pink slips' saving: a car saved with its dialogs, the second card's player's name; the
/// memory cards' start and end; the texts (saving to the first card and the second, the other
/// player's card).
const SAVE_PINK_SLIPS: fn(&mut Cpu) = crate::fememcard::save_pink_slips_with_dialogs_entry;
const PLAYER_NAME: fn(&mut Cpu) = crate::front::player_name_entry;
const CARD_START: fn(&mut Cpu) = crate::fememcard::init_memcard_entry;
const CARD_END: fn(&mut Cpu) = crate::fememcard::deinit_memcard_entry;
const SAVING_TEXT: u32 = 662;
const OTHER_CARD_TEXT: u32 = 664;
/// The front end idle this long (ticks) runs the demo video.
const IDLE: i32 = 3840;
/// A menu's flags choosing whose keys it takes: both players' (also with two players racing,
/// unless it takes only its own player's); the player who pressed a key last; the first's; the
/// second's.
const MENU_BOTH_PLAYERS_KEYS: u32 = 0x10;
const MENU_OWN_PLAYERS_KEYS: u32 = 0x08;
const MENU_INPUT_PLAYERS_KEYS: u32 = 0x20;
const MENU_FIRST_PLAYERS_KEYS: u32 = 0x40;
const MENU_SECOND_PLAYERS_KEYS: u32 = 0x80;

/// `MainLoop`'s locals the game takes by address: each player's command (8 bytes) and key, the
/// car raced (a `tCarInfo`), the pink slips' message.
#[derive(Clone, Copy)]
struct Locals(u32);

impl Locals {
    const SIZE: u32 = 512;

    fn command(self, k: u32) -> MenuCommand {
        MenuCommand(self.0 + 8 * k)
    }

    fn key(self, k: u32) -> u32 {
        self.0 + 16 + 4 * k
    }

    fn car(self) -> CarInfo {
        CarInfo(self.0 + 24)
    }

    fn text(self) -> u32 {
        self.0 + 24 + ((CarInfo::SIZE + 7) & !7)
    }
}

/// What `MainLoop` keeps from frame to frame.
struct Loop {
    this: FEApplication,
    locals: Locals,
    /// When each player's key was last taken.
    key_times: [u32; 2],
    /// The first player's back depth when the second player's menu came up beside it (-1: none
    /// up).
    second_depth: i16,
    /// The second player's menu to come up once the first's is on.
    second_pending: bool,
    /// Whether the frame is drawn.
    draw: bool,
    /// The frame's start, and the last time a player did something.
    frame_start: u32,
    last_active: u32,
}

/// `tFEApplication::MainLoop` (FEAPP.CPP, 0x800143c8): the front end from `first`. Each frame,
/// for each player: the next menu, once the one going off is done; the second player's menu
/// gone once it is off; the next screen, once the one going off is done; the keys of the
/// players the menu listens to, and their commands. Then the frame is drawn, and the demo video
/// runs after a minute idle. Returns 0 for a race, 2 to quit.
pub fn main_loop(c: &mut Cpu, this: FEApplication, first: Menu) -> u32 {
    with_buffer(c, Locals::SIZE, |c, at| {
        let mut s = Loop {
            this,
            locals: Locals(at),
            key_times: [0; 2],
            second_depth: -1,
            second_pending: false,
            draw: false,
            frame_start: 0,
            last_active: 0,
        };
        this.set_input_player(&mut c.m, 0);
        perform_menu_initialization(c, this);
        let m = &mut c.m;
        s.last_active = m.r32(GAME_TICKS);
        m.w32(FLIP, 0);
        for p in 0..2 {
            this.set_back_depth(m, p, 0);
            this.set_current_menu(m, p, Menu(0));
            this.set_current_screen(m, p, Screen(0));
            this.set_transition_to_menu(m, p, Menu(0));
            this.set_transition_to_screen(m, p, Screen(0));
            this.set_waiting_for_other_player(m, p, 0);
            this.set_last_key_pressed(m, p, 0);
            this.set_parent_menu(m, p, Menu(0));
        }
        this.set_player(m, 2);
        set_menu(c, this, 0, first);
        loop {
            s.draw = true;
            s.frame_start = c.m.r32(GAME_TICKS);
            this.set_player(&mut c.m, 0);
            while player(&mut c.m, this) < 2 {
                next_menu(c, &mut s);
                if player(&mut c.m, this) == 1 {
                    second_menu_gone(c, &s);
                }
                next_screen(c, this);
                if let Some(result) = input(c, &mut s) {
                    return result;
                }
                let p = player(&mut c.m, this);
                this.set_player(&mut c.m, (p + 1) as i8);
            }
            if s.draw {
                call_addr(c, REDRAW, &[this.0]);
            }
            if s.frame_start.wrapping_sub(s.last_active) as i32 > IDLE {
                run_demo_video(c, this);
                s.last_active = c.m.r32(GAME_TICKS);
            }
        }
    })
}

pub fn abi_main_loop(c: &mut Cpu) {
    c.r[2] = main_loop(c, FEApplication(c.r[4]), Menu(c.r[5]));
}

/// `MainLoop`'s next menu for the player, once the menu now is done going off (and its screen),
/// or at once with none. A sub-menu leaving without a new screen leaves the back stack one
/// shorter and the frame undrawn; otherwise the next menu comes on (with the second player's
/// beside it, when one is pending). The new menu's parent is the old one when it is a sub-menu.
fn next_menu(c: &mut Cpu, s: &mut Loop) {
    let this = s.this;
    let p = player(&mut c.m, this);
    if this.transition_to_menu(&mut c.m, p).0 == 0 {
        return;
    }
    let now = this.current_menu(&mut c.m, p);
    let mut sub = false;
    if now.0 != 0 {
        let off = menu_fn(c, now, MENU_TRANSITION_IS_FINISHED) != 0 && {
            let screen = current_screen(&mut c.m, this);
            screen.0 != 0 && screen_fn(c, screen, SCREEN_TRANSITION_IS_FINISHED) != 0
        };
        if !off {
            return;
        }
        let now = current_menu(&mut c.m, this);
        sub = now.0 != 0 && menu_fn(c, now, MENU_IS_SUB_MENU) != 0;
    }
    let p = player(&mut c.m, this);
    let next = this.transition_to_menu(&mut c.m, p);
    let next_is_sub = menu_fn(c, next, MENU_IS_SUB_MENU) != 0;
    let m = &mut c.m;
    let p = player(m, this);
    let parent = if next_is_sub { this.current_menu(m, p) } else { Menu(0) };
    this.set_parent_menu(m, p, parent);
    let next = this.transition_to_menu(m, p);
    this.set_current_menu(m, p, next);
    menu_fn(c, next, MENU_INITIALIZE);
    let m = &mut c.m;
    let p = player(m, this);
    this.set_transition_to_menu(m, p, Menu(0));
    // (the first player's screen checked, whichever player this is: as the original does)
    if sub && this.transition_to_screen(m, 0).0 == 0 {
        this.update_back_depth(m, p, |d| d - 1);
        s.draw = false;
    } else {
        let menu = this.current_menu(m, p);
        menu_fn(c, menu, MENU_TRANSITION_ON);
        if s.second_pending {
            let second = current_menu(&mut c.m, this).child_menu(&mut c.m);
            s.second_pending = false;
            set_menu(c, this, 1, second);
        }
    }
    s.last_active = c.m.r32(GAME_TICKS);
}

/// `MainLoop`'s second player's menu gone (and its screen cleaned up) once it is off, when it
/// is no longer up beside the first's, and its parent is off too.
fn second_menu_gone(c: &mut Cpu, s: &Loop) {
    let this = s.this;
    let menu = this.current_menu(&mut c.m, 1);
    if menu.0 == 0 || menu_fn(c, menu, MENU_TRANSITION_IS_FINISHED) == 0 || s.second_depth >= 0 {
        return;
    }
    let parent = current_parent(&mut c.m, this);
    if parent.0 != 0 && menu_fn(c, parent, MENU_TRANSITION_IS_FINISHED) == 0 {
        return;
    }
    let p = player(&mut c.m, this);
    this.set_current_menu(&mut c.m, p, Menu(0));
    let screen = current_screen(&mut c.m, this);
    if screen.0 != 0 {
        screen_fn(c, screen, SCREEN_CLEANUP);
    }
    let p = player(&mut c.m, this);
    this.set_current_screen(&mut c.m, p, Screen(0));
}

/// `MainLoop`'s next screen for the player, once the screen now is done going off (cleaned up,
/// unless a menu is still to come), or at once with none: initialised (the memory left noted
/// first) and coming on.
fn next_screen(c: &mut Cpu, this: FEApplication) {
    let p = player(&mut c.m, this);
    if this.transition_to_screen(&mut c.m, p).0 == 0 {
        return;
    }
    let now = this.current_screen(&mut c.m, p);
    if now.0 != 0 {
        if screen_fn(c, now, SCREEN_TRANSITION_IS_FINISHED) == 0 {
            return;
        }
        let p = player(&mut c.m, this);
        if this.transition_to_menu(&mut c.m, p).0 != 0 {
            return;
        }
        let now = this.current_screen(&mut c.m, p);
        if now.0 != 0 {
            screen_fn(c, now, SCREEN_CLEANUP);
        }
    }
    let p = player(&mut c.m, this);
    let next = this.transition_to_screen(&mut c.m, p);
    this.set_current_screen(&mut c.m, p, next);
    let left = call(c, LARGEST_UNUSED, &[]);
    c.m.w32(MEMORY_LEFT, left);
    let screen = current_screen(&mut c.m, this);
    screen_fn(c, screen, SCREEN_INITIALIZE);
    let p = player(&mut c.m, this);
    this.set_transition_to_screen(&mut c.m, p, Screen(0));
    let screen = current_screen(&mut c.m, this);
    call_addr(c, SCREEN_TRANSITION_ON, &[screen.0, TRANSITION, 0]);
}

/// `MainLoop`'s input for the player, once its menu and screen are on: the keys of the players
/// the menu's flags choose, from the first to the last, and their commands. `Some` the loop's
/// result when a race starts or the front end is quit.
fn input(c: &mut Cpu, s: &mut Loop) -> Option<u32> {
    let this = s.this;
    let menu = current_menu(&mut c.m, this);
    if menu.0 == 0 {
        return None;
    }
    let m = &mut c.m;
    let flags = menu.flags(m);
    let p = player(m, this);
    let (mut first, mut last) = (p, p);
    if flags & MENU_BOTH_PLAYERS_KEYS != 0 || (m.r8(TWO_PLAYERS) == 1 && flags & MENU_OWN_PLAYERS_KEYS == 0) {
        (first, last) = (0, 1);
    }
    if flags & MENU_INPUT_PLAYERS_KEYS != 0 {
        let k = this.input_player(m) as u8 as u32;
        (first, last) = (k, k);
    }
    if flags & MENU_FIRST_PLAYERS_KEYS != 0 {
        (first, last) = (0, 0);
    }
    if flags & MENU_SECOND_PLAYERS_KEYS != 0 {
        (first, last) = (1, 1);
    }
    let on = menu_fn(c, menu, MENU_TRANSITION_IS_FINISHED) != 0 && {
        let screen = current_screen(&mut c.m, this);
        screen.0 != 0 && screen_fn(c, screen, SCREEN_TRANSITION_IS_FINISHED) != 0
    };
    if !on {
        return None;
    }
    let menu = current_menu(&mut c.m, this);
    let debounced = menu_fn(c, menu, MENU_DEBOUNCE_KEYS);
    for k in first..=last {
        key(c, s, k, debounced);
        if let Some(result) = command(c, s, k) {
            return Some(result);
        }
    }
    None
}

/// Player `k`'s key (those in `debounced` only when pressed again), handed to the help (help
/// hides it), the top-most dialog, the screen and the menu, which may leave a command.
fn key(c: &mut Cpu, s: &mut Loop, k: u32, debounced: u32) {
    let (this, l) = (s.this, s.locals);
    l.command(k).set_type_(&mut c.m, 0);
    let key = call_addr(c, GET_KEY_FROM_PLAYER, &[k, debounced]);
    let m = &mut c.m;
    let at = l.key(k);
    m.w32(at, key);
    if key != 0 {
        this.set_input_player(m, k as i8);
    }
    // the last key, unless a debounced one held down
    if m.r32(GAME_TICKS).wrapping_sub(s.key_times[k as usize]) >= KEY_REPEAT || debounced & m.r32(at) == 0 {
        let key = m.r32(at);
        this.set_last_key_pressed(m, k, key as i32);
    }
    if m.r32(at) == 0 {
        return;
    }
    let top = call_addr(c, GET_TOP_MOST_DIALOG, &[]);
    s.last_active = s.frame_start;
    s.key_times[k as usize] = s.frame_start;
    let help = this.help_popup();
    if c.m.r32(at) == KEY_HELP && help.currently_on(&mut c.m) != 0 {
        c.m.w32(at, KEY_NONE);
        call_addr(c, HIDE, &[help.0]);
    }
    let command = l.command(k).0;
    if top != 0 && c.m.r32(at) != KEY_HELP {
        call_virtual(c, top, SCREEN_VPTR, SCREEN_PROCESS_INPUT, &[k, at, command]);
    }
    let screen = current_screen(&mut c.m, this);
    if screen.0 != 0 {
        call_virtual(c, screen.0, SCREEN_VPTR, SCREEN_PROCESS_INPUT, &[k, at, command]);
    }
    if c.m.r32(at) != KEY_NONE {
        let menu = current_menu(&mut c.m, this);
        call_virtual(c, menu.0, MENU_VPTR, MENU_PROCESS_INPUT, &[k, at, command]);
    }
}

/// The current menu of the player pushed on its back stack.
fn push(m: &mut Mem, this: FEApplication, p: u32) {
    let depth = this.back_depth(m, p) as u32;
    let menu = this.current_menu(m, p);
    this.set_back_list(m, 16 * p + depth, menu);
}

/// Player `k`'s command, run for the player being run: `Some` the loop's result when a race
/// starts or the front end is quit.
fn command(c: &mut Cpu, s: &mut Loop, k: u32) -> Option<u32> {
    let (this, command) = (s.this, s.locals.command(k));
    let p = player(&mut c.m, this);
    let kind = command.type_(&mut c.m) as u32;
    match kind {
        COMMAND_GO_TO | COMMAND_GO_TO_RESET => {
            call(c, PLAY_FE_SFX, &[SOUND_ACCEPT]);
            let m = &mut c.m;
            let p = player(m, this);
            if kind == COMMAND_GO_TO {
                push(m, this, p);
                this.update_back_depth(m, p, |d| d + 1);
            } else {
                this.set_back_depth(m, p, 0);
            }
            let next = command.next_menu(m);
            set_menu(c, this, p, next);
        }
        COMMAND_GO_TO_WITH_SECOND => {
            call(c, PLAY_FE_SFX, &[SOUND_ACCEPT]);
            let m = &mut c.m;
            let p = player(m, this);
            push(m, this, p);
            s.second_pending = true;
            this.update_back_depth(m, p, |d| d + 1);
            s.second_depth = this.back_depth(m, p) as i16;
            let next = command.next_menu(m);
            set_menu(c, this, p, next);
            this.set_back_depth(&mut c.m, 1, 0);
        }
        COMMAND_BACK => back(c, s),
        COMMAND_RACE_WHEN_BOTH_READY => {
            let m = &mut c.m;
            let other = 1 - p;
            if this.waiting_for_other_player(m, other) == 0 {
                this.set_waiting_for_other_player(m, p, 1);
            } else {
                this.set_waiting_for_other_player(m, other, 0);
                if m.r8(RACE_TYPE) as u32 != PINK_SLIPS || save_pink_slips(c, this, s.locals.text()) {
                    return Some(start_race(c, this, s.locals.car()));
                }
            }
        }
        COMMAND_RACE | COMMAND_RACE_TOO => return Some(start_race(c, this, s.locals.car())),
        COMMAND_QUIT => {
            c.m.w32(GAME_START, 2);
            perform_menu_destruction(c, this);
            return Some(2);
        }
        _ => {}
    }
    None
}

/// `MainLoop`'s back. A player waiting for the other with no menu to go back to stops waiting.
/// Otherwise both stop waiting, and:
/// - the first player back where the second player's menu came up: the second's menus and
///   screen go off;
/// - the second player at the bottom of its stack: its menus and screen go off, and the first
///   player goes back to the menu before;
/// - else the player goes one menu back down its stack (or, the second player with none left,
///   just the sound).
fn back(c: &mut Cpu, s: &mut Loop) {
    let this = s.this;
    let m = &mut c.m;
    let p = player(m, this);
    let down = match p {
        0 => (s.second_depth as i32) < this.back_depth(m, 0),
        1 => this.back_depth(m, 1) > 0,
        _ => false,
    };
    if !down {
        if this.waiting_for_other_player(m, p) != 0 {
            this.set_waiting_for_other_player(m, p, 0);
            return;
        }
        this.set_waiting_for_other_player(m, 1 - p, 0);
        this.set_waiting_for_other_player(m, p, 0);
    }
    let parents_off = |c: &mut Cpu| {
        for p in 0..2 {
            let parent = this.parent_menu(&mut c.m, p);
            if parent.0 != 0 {
                menu_fn(c, parent, MENU_TRANSITION_OFF);
            }
        }
    };
    let second_screen_off = |c: &mut Cpu| {
        let screen = this.current_screen(&mut c.m, 1);
        call_addr(c, SCREEN_TRANSITION_OFF, &[screen.0, TRANSITION, 0]);
    };
    let m = &mut c.m;
    if p == 0 && this.back_depth(m, 0) == s.second_depth as i32 && this.current_menu(m, 1).0 != 0 {
        // the second player's menus going off
        parents_off(c);
        let menu = this.current_menu(&mut c.m, 1);
        s.second_depth = -1;
        menu_fn(c, menu, MENU_TRANSITION_OFF);
        second_screen_off(c);
        this.set_back_depth(&mut c.m, 1, 0);
    } else if player(m, this) == 1 {
        if this.back_depth(m, 1) > 0 {
            back_down(c, this);
            return;
        }
        if this.current_menu(m, 1).0 != 0 {
            // the second player's menus going off, the first player back to the menu before
            this.set_back_depth(m, 0, s.second_depth as i32 - 1);
            let menu = this.current_menu(m, 1);
            menu_fn(c, menu, MENU_TRANSITION_OFF);
            parents_off(c);
            s.second_depth = -1;
            second_screen_off(c);
            let m = &mut c.m;
            let depth = this.back_depth(m, 0) as u32;
            let menu = this.back_list(m, depth);
            set_menu(c, this, 0, menu);
        }
    }
    let m = &mut c.m;
    let p = player(m, this);
    if this.back_depth(m, p) > 0 {
        back_down(c, this);
    } else if p == 1 {
        call(c, PLAY_FE_SFX, &[SOUND_BACK]);
    }
}

/// One menu back down the player's stack, with a sound (the depth kept when the menu gone back
/// to is a sub-menu).
fn back_down(c: &mut Cpu, this: FEApplication) {
    call(c, PLAY_FE_SFX, &[SOUND_BACK]);
    let m = &mut c.m;
    let p = player(m, this);
    this.update_back_depth(m, p, |d| d - 1);
    let depth = this.back_depth(m, p) as u32;
    let menu = this.back_list(m, 16 * p + depth);
    set_menu(c, this, p, menu);
    let menu = current_menu(&mut c.m, this);
    if menu_fn(c, menu, MENU_IS_SUB_MENU) != 0 {
        let p = player(&mut c.m, this);
        this.update_back_depth(&mut c.m, p, |d| d + 1);
    }
}

/// The two players' pink slips race: each player's car saved to the memory card at stake (on
/// the second card, the first player told by name whose card it is). Whether both were saved;
/// if not, the music is back on and the front end goes on.
fn save_pink_slips(c: &mut Cpu, this: FEApplication, text: u32) -> bool {
    call(c, STOP_SONG, &[400]);
    call(c, CARD_START, &[0, 1]);
    let dialog = |m: &mut Mem| FEApplication(m.r32(FE_APP)).no_input_mem_card_dialog();
    let mut failed = 0;
    for k in 0..2 {
        if failed != 0 {
            break;
        }
        let word = call(c, TEXT_WORD, &[SAVING_TEXT + k]);
        dialog(&mut c.m).set_string(&mut c.m, word);
        let d = dialog(&mut c.m);
        call_addr(c, DISPLAY, &[d.0]);
        while dialog(&mut c.m).fully_open(&mut c.m) != 1 {
            let app = c.m.r32(FE_APP);
            call_addr(c, REDRAW, &[app]);
        }
        let car = c.m.r8(PINK_SLIPS_CARS + k) as u32;
        failed = call(c, SAVE_PINK_SLIPS, &[k, 0, car]);
        if failed != 0 && k == 1 {
            let format = call(c, TEXT_WORD, &[OTHER_CARD_TEXT]);
            let name = call(c, PLAYER_NAME, &[0]);
            call(c, SPRINTF, &[text, format, name, 1]);
            dialog(&mut c.m).set_string(&mut c.m, text);
            call(c, SAVE_PINK_SLIPS, &[0, 1, u32::MAX]);
        }
        let d = dialog(&mut c.m);
        call_addr(c, HIDE, &[d.0]);
    }
    call(c, CARD_END, &[]);
    if failed == 0 {
        return true;
    }
    update_music(c, this);
    let volume = music_volume(&mut c.m);
    call(c, MUSIC_VOLUME, &[volume]);
    false
}

/// A race started: the player's car got into `car` (the pink slips', a stock car or the
/// garage's), its engine's sound, 256 ticks for the sound, the menus' end. Returns 0.
fn start_race(c: &mut Cpu, this: FEApplication, car: CarInfo) -> u32 {
    let m = &mut c.m;
    let p = player(m, this);
    if m.r8(RACE_TYPE) as u32 == PINK_SLIPS {
        let k = m.r8(PINK_SLIPS_CARS + p) as u32;
        call_addr(c, GET_PINK_SLIPS_CAR, &[CAR_MANAGER, k, car.0, p]);
    } else if m.r8(GARAGE_CAR) == 0 {
        let k = m.r8(STOCK_CARS + p) as u32;
        call_addr(c, GET_STOCK_CAR, &[CAR_MANAGER, k, car.0]);
    } else {
        let k = m.r8(GARAGE_CARS + p) as u32;
        call_addr(c, GET_GARAGE_CAR, &[CAR_MANAGER, k, car.0, p]);
    }
    let sound = car.engine_patch(&mut c.m) as u32;
    if sound != 0 {
        call(c, PLAY_FE_SFX, &[sound]);
    }
    let from = c.m.r32(GAME_TICKS);
    while c.m.r32(GAME_TICKS).wrapping_sub(from) < 256 {
        call(c, SYSTEM_TASK, &[0]);
    }
    c.m.w32(GAME_START, 0);
    perform_menu_destruction(c, this);
    0
}

/// The demo video's number (5 and 0 alternate); the audio's car sounds' end, its driver's end
/// and start; the front end's samples loaded.
const DEMO_VIDEO: u32 = 0x8005_19A4;
const CAR_SOUNDS_END: fn(&mut Cpu) = nfs4rc::f_800bcb04;
const AUDIO_DEINIT: fn(&mut Cpu) = nfs4rc::f_800bca4c;
const AUDIO_INIT: fn(&mut Cpu) = nfs4rc::f_800bc958;
const LOAD_FE_SAMPLES: fn(&mut Cpu) = nfs4rc::f_800780f0;
/// The front end's graphics and data: draw memory freed and allocated, FETOOLS' end and
/// start, the loading text, the help's shapes loaded; the movie played.
const FREE_DRAW_MEMORY: u32 = 0x8004_DD18;
const ALLOCATE_DRAW_MEMORY: u32 = 0x8004_DCA4;
const FETOOLS_DEINIT: u32 = 0x8002_6CB0;
const FETOOLS_INIT: u32 = 0x8002_6BC8;
const DISPLAY_LOADING_TEXT: u32 = 0x8002_5AD8;
const LOAD_ALL_HELP_SHAPES: u32 = 0x8004_F09C;
const PLAY_MOVIE: u32 = 0x8004_D3DC;
/// `ClearImage` and `DrawSync`.
const CLEAR_IMAGE: u32 = 0x800E_E35C;
const DRAW_SYNC: u32 = 0x800E_E1D8;

/// VRAM cleared (1024 by 512, black) and drawn.
fn clear_vram(c: &mut Cpu) {
    with_buffer(c, 8, |c, at| {
        let (rect, m) = (RECT(at), &mut c.m);
        rect.set_x(m, 0);
        rect.set_y(m, 0);
        rect.set_w(m, 1024);
        rect.set_h(m, 512);
        call_addr(c, CLEAR_IMAGE, &[at, 0, 0, 0]);
        call_addr(c, DRAW_SYNC, &[0]);
    })
}

/// The memory left noted.
fn note_memory_left(c: &mut Cpu) {
    let left = call(c, LARGEST_UNUSED, &[]);
    c.m.w32(MEMORY_LEFT, left);
}

/// `tFEApplication::RunDemoVideo` (FEAPP.CPP, 0x800140e8), from the main menu only. The music
/// stops; the menu and the screen go off (frames drawn until both are done). The screen is
/// cleaned up, the front end's graphics and sounds freed, and the demo video played over
/// cleared VRAM. Then everything comes back (the graphics, the sounds, the loading text, the
/// help's shapes), the menu and the screen initialised and coming on, the other demo video next
/// time, the music at its volume.
pub fn run_demo_video(c: &mut Cpu, this: FEApplication) {
    let main = Menu(c.m.r32(MENU_DEFS).wrapping_add(MAIN_MENU));
    if this.current_menu(&mut c.m, 0) != main {
        return;
    }
    call(c, STOP_SONG, &[120]);
    call(c, SYSTEM_TASK, &[0]);
    let menu = this.current_menu(&mut c.m, 0);
    menu_fn(c, menu, MENU_TRANSITION_OFF);
    let screen = this.current_screen(&mut c.m, 0);
    call_addr(c, SCREEN_TRANSITION_OFF, &[screen.0, TRANSITION, 0]);
    loop {
        let menu = this.current_menu(&mut c.m, 0);
        let off = menu_fn(c, menu, MENU_TRANSITION_IS_FINISHED) != 0 && {
            let screen = this.current_screen(&mut c.m, 0);
            screen_fn(c, screen, SCREEN_TRANSITION_IS_FINISHED) != 0
        };
        if off {
            break;
        }
        call_addr(c, REDRAW, &[this.0]);
        call(c, SYSTEM_TASK, &[0]);
        c.tick();
    }
    let screen = this.current_screen(&mut c.m, 0);
    screen_fn(c, screen, SCREEN_CLEANUP);
    call(c, CAR_SOUNDS_END, &[]);
    call(c, AUDIO_DEINIT, &[]);
    call_addr(c, FREE_DRAW_MEMORY, &[]);
    call_addr(c, FETOOLS_DEINIT, &[]);
    free_help_shape_cluts(c);
    note_memory_left(c);
    clear_vram(c);
    let k = c.m.r8(DEMO_VIDEO) as u32;
    call_addr(c, PLAY_MOVIE, &[k]);
    clear_vram(c);
    note_memory_left(c);
    call_addr(c, ALLOCATE_DRAW_MEMORY, &[]);
    call_addr(c, FETOOLS_INIT, &[]);
    call_addr(c, DISPLAY_LOADING_TEXT, &[]);
    call(c, AUDIO_INIT, &[0xD800, 0x1_8000]);
    call(c, LOAD_FE_SAMPLES, &[]);
    call_addr(c, LOAD_ALL_HELP_SHAPES, &[]);
    note_memory_left(c);
    let menu = this.current_menu(&mut c.m, 0);
    menu_fn(c, menu, MENU_INITIALIZE);
    let screen = this.current_screen(&mut c.m, 0);
    screen_fn(c, screen, SCREEN_INITIALIZE);
    let menu = this.current_menu(&mut c.m, 0);
    menu_fn(c, menu, MENU_TRANSITION_ON);
    let screen = this.current_screen(&mut c.m, 0);
    call_addr(c, SCREEN_TRANSITION_ON, &[screen.0, TRANSITION, 0]);
    let k = c.m.r32(DEMO_VIDEO);
    c.m.w32(DEMO_VIDEO, 5u32.wrapping_sub(k));
    update_music(c, this);
    let volume = music_volume(&mut c.m);
    call(c, MUSIC_VOLUME, &[volume]);
}

pub fn abi_run_demo_video(c: &mut Cpu) {
    run_demo_video(c, FEApplication(c.r[4]));
}

/// The race's cars' stats; the track raced; the records: new ones cleared, whether a car's
/// lap is a record (the car in `*who`) and whether the race made top times (bits: the first
/// player's race 1 and lap 2, the second's race 4 and lap 8). The pads reset (with their
/// kinds, `frontEnd + 866`); the post-game menu.
const RACE_STATS: u32 = 0x8011_0E08;
const TRACK_RACED: u32 = 0x8013_DAEC;
const CLEAR_NEW_RECORDS: u32 = 0x8004_AD14;
const IS_RECORD_LAP_TIME: u32 = 0x8004_9CC8;
const IS_TOP_TIME: u32 = 0x8004_A194;
const RESET_PSX_CONTROLLER: u32 = 0x8002_7104;
const PAD_KINDS: u32 = FRONT_END + 866;
const POST_GAME_MENU: u32 = 0x8002_DBC4;
/// The name entries: the players' names, the name screen (its menu at +100), and each player's
/// name grid and menu (FEMENUDEFS' menus' offsets).
const NAMES: u32 = 0x8011_5C1C;
const NAME_SCREEN: u32 = 0x8005_2E44;
const NAME_ENTRIES: [(u32, u32); 2] = [(12728, 14032), (12868, 14160)];

/// The name entry for player `p`: its grid set for the player's name (7 long, at its start),
/// the name screen going to its menu, which is returned.
fn name_entry(m: &mut Mem, p: u32) -> Menu {
    let defs = m.r32(MENU_DEFS);
    let (grid, menu) = (defs + NAME_ENTRIES[p as usize].0, defs + NAME_ENTRIES[p as usize].1);
    m.w16(grid + 128, p as u16);
    m.w32(grid + 28, NAMES + 8 * p);
    let screen = m.r32(NAME_SCREEN);
    m.w16(grid + 32, 7);
    m.w16(grid + 36, 0);
    m.w16(grid + 34, 0);
    m.w32(screen + 100, menu);
    Menu(menu)
}

/// `tFEApplication::RunPostGame` (FEAPP.CPP, 0x80015554). After a race (not a pink slips' one,
/// nor one of race type 0 with a stock car), the new records are cleared, both pads reset, and
/// no names are wanted. A record lap, then top times, are noted for each player as a name to
/// enter and a speech to play:
/// - one player: a lap record 6; a top race 3 or lap 0 (12 or 9 after a lap record);
/// - two players: a lap record 7 (the first player's) or 8 (the second's); the first player's
///   top race 4 or lap 1 (13 or 10 after its own lap record), the second's top race 5 or lap 2
///   (14 or 11 after its own lap record).
///
/// Then the front end runs from the first player's name entry, the second's, or the post-game
/// menu.
pub fn run_post_game(c: &mut Cpu, this: FEApplication) -> u32 {
    let kind = c.m.r8(RACE_TYPE) as u32;
    let mut menu = None;
    if kind != PINK_SLIPS && !(kind == 0 && c.m.r8(GARAGE_CAR) == 0) {
        call_addr(c, CLEAR_NEW_RECORDS, &[]);
        for p in 0..2u32 {
            let pad = c.m.r8(PAD_KINDS + p) as u32;
            call_addr(c, RESET_PSX_CONTROLLER, &[p, pad]);
            let m = &mut c.m;
            this.set_got_name(m, p, 0);
            this.set_need_name(m, p, 0);
            this.set_speech_to_play(m, p, -1);
        }
        // (the record lap's car is written only when a car is checked: the original reads its
        // stack's leftover then; here it starts at 0)
        let (record, who) = with_buffer(c, 8, |c, at| {
            c.m.w16(at, 0);
            let track = c.m.r16(TRACK_RACED) as i16 as i32 as u32;
            let record = call_addr(c, IS_RECORD_LAP_TIME, &[RACE_STATS, track, at]) != 0;
            (record, c.m.r16(at) as i16 as i32)
        });
        let track = c.m.r16(TRACK_RACED) as i16 as i32 as u32;
        let top = sx(call_addr(c, IS_TOP_TIME, &[RACE_STATS, track]));
        let m = &mut c.m;
        let two = m.r8(TWO_PLAYERS) == 1;
        if record {
            let who = who as u32;
            let speech = if two { if who != 0 { 8 } else { 7 } } else { 6 };
            this.set_speech_to_play(m, who, speech);
            this.set_need_name(m, who, 1);
        }
        if top != 0 {
            let pick = |bit: u32, (yes, no): (i32, i32)| if top & bit != 0 { yes } else { no };
            if two {
                if top & 3 != 0 {
                    this.set_need_name(m, 0, 1);
                    let first = this.speech_to_play(m, 0) == -1 || who != 0;
                    this.set_speech_to_play(m, 0, if first { pick(2, (1, 4)) } else { pick(2, (10, 13)) });
                }
                if top & 0xC != 0 {
                    this.set_need_name(m, 1, 1);
                    let after = this.speech_to_play(m, 1) != -1 && who == 1;
                    this.set_speech_to_play(m, 1, if after { pick(8, (11, 14)) } else { pick(8, (2, 5)) });
                }
            } else {
                this.set_need_name(m, 0, 1);
                let first = this.speech_to_play(m, 0) == -1;
                this.set_speech_to_play(m, 0, if first { pick(2, (0, 3)) } else { pick(2, (9, 12)) });
            }
        }
        if this.need_name(m, 0) != 0 {
            menu = Some(name_entry(m, 0));
        } else if this.need_name(m, 1) != 0 {
            menu = Some(name_entry(m, 1));
        }
    }
    let menu = match menu {
        Some(menu) => menu,
        None => with_buffer(c, 8, |c, at| {
            call_addr(c, POST_GAME_MENU, &[at]);
            MenuCommand(at).next_menu(&mut c.m)
        }),
    };
    main_loop(c, this, menu)
}

pub fn abi_run_post_game(c: &mut Cpu) {
    c.r[2] = run_post_game(c, FEApplication(c.r[4]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, MENU, MENU2, NEW, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// The application's size (to its last field, +896) and where a copy goes.
    const APP_SIZE: u32 = 900;
    const COPY: u32 = 0x8014_4000;
    /// The pads' library state (`PAD_update` copies it), see FEINPUT's tests.
    const PAD_GLOBAL: u32 = 0x8013_FBF8;
    /// Routines returning 0 and 1 (interpreted), the test menus' vtables copied.
    const STUBS: u32 = 0x8014_7800;
    const FAKE_VTS: u32 = 0x8014_7900;

    fn app(m: &mut Mem) -> u32 {
        m.r32(FE_APP)
    }

    /// FEMENU's structures, then the pads plugged in or not and their kinds; the game's
    /// application with each player's menus (the game's, FEMENU's test menus, none for the
    /// second) and their options and vertical flags, the menus left and overlaid, the half
    /// drawn; the music's flags; a copy of it.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        for p in 0..2 {
            let status = (pick(rng, 3) == 0) as u8;
            let kind = [0x41, 0x73, 0x23][pick(rng, 3) as usize] as u8;
            m.w8(PAD_INFO + 32 * p + 4, status);
            m.w8(PAD_INFO + 32 * p + 5, kind);
            m.w8(PAD_GLOBAL + 34 * p, status);
            m.w8(PAD_GLOBAL + 34 * p + 1, kind);
        }
        let a = app(m);
        let app = FEApplication(a);
        let real = app.current_menu(m, 0).0;
        app.set_current_menu(m, 0, Menu([real, real, MENU, MENU2][pick(rng, 4) as usize]));
        app.set_current_menu(m, 1, Menu([0, 0, MENU2][pick(rng, 3) as usize]));
        for menu in [MENU, MENU2] {
            m.w16(menu + 100, (pick(rng, 3) == 0) as u16);
        }
        app.set_parent_menu(m, 0, Menu([0, MENU2][pick(rng, 2) as usize]));
        for p in 0..2 {
            app.set_waiting_for_other_player(m, p, pick(rng, 2));
        }
        // (the test menus' transitions answering either way: their vtables copied, the two
        // transition calls made routines returning 0 or 1)
        for (k, w) in [0x03E0_0008, 0x0000_1021, 0x03E0_0008, 0x2402_0001].into_iter().enumerate() {
            m.w32(STUBS + 4 * k as u32, w);
        }
        for (menu, vt) in [(MENU, FAKE_VTS), (MENU2, FAKE_VTS + 0x100)] {
            let real_vt = m.r32(menu + MENU_VPTR);
            for k in 0..32 {
                let v = m.r32(real_vt + 4 * k);
                m.w32(vt + 4 * k, v);
            }
            for slot in [MENU_TRANSITION_IS_FINISHED, MENU_IS_SUB_MENU] {
                let f = m.r32(real_vt + slot + 4);
                m.w32(vt + slot + 4, [f, STUBS, STUBS + 8][pick(rng, 3) as usize]);
            }
            m.w32(menu + MENU_VPTR, vt);
        }
        app.set_current_music(m, [0, 0x1000, 0x2000][pick(rng, 3) as usize]);
        app.set_player(m, pick(rng, 2) as i8);
        for k in 0..APP_SIZE {
            let b = m.r8(a + k);
            m.w8(COPY + k, b);
        }
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tFEApplication", 0x8001_30f8, nfs4rc::f_800130f8, |c, _| {
                for k in 0..APP_SIZE / 4 {
                    c.m.w32(NEW + 4 * k, 0);
                }
                vec![NEW]
            }),
            ("~tFEApplication", 0x8001_331c, nfs4rc::f_8001331c, |_, r| vec![COPY, 2 * pick(r, 2)]),
            ("PerformMenuInitialization", 0x8001_338c, nfs4rc::f_8001338c, |c, _| vec![app(&mut c.m)]),
            ("PerformMenuDestruction", 0x8001_33f4, nfs4rc::f_800133f4, |c, _| vec![app(&mut c.m)]),
            ("DrawHelpIcons", 0x8001_349c, nfs4rc::f_8001349c, |c, _| vec![app(&mut c.m)]),
            ("Redraw", 0x8001_3894, nfs4rc::f_80013894, |c, _| vec![app(&mut c.m)]),
            ("SetMenu", 0x8001_3ef4, nfs4rc::f_80013ef4, |c, r| {
                let a = app(&mut c.m);
                let now = FEApplication(a).current_menu(&mut c.m, 0).0;
                vec![a, pick(r, 2), [now, MENU, MENU2][pick(r, 3) as usize]]
            }),
            ("SetScreen", 0x8001_3ff8, nfs4rc::f_80013ff8, |c, r| {
                let a = app(&mut c.m);
                let app = FEApplication(a);
                let now = app.current_screen(&mut c.m, 0).0;
                vec![a, pick(r, 2), [0, now, app.message_popup().0][pick(r, 3) as usize]]
            }),
            ("DisplayHelp", 0x8001_4048, nfs4rc::f_80014048, |c, r| vec![app(&mut c.m), pick(r, 3)]),
            ("FreeHelpShapeCluts", 0x8001_406c, nfs4rc::f_8001406c, |_, _| vec![]),
        ]
    }

    #[test]
    fn feapp_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xA990 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 150, 0xA991 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
