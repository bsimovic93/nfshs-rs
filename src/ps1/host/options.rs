//! The game options screen as the rebuild has it: Graphics in place of Memory Card (the game
//! saves by itself, see [`Autosave`]). Graphics is a screen like the original's Display one,
//! built from its pieces, whose choices are the host's settings (window, resolution,
//! antialiasing, texture filter, CRT filter, bloom).
//!
//! The front end's menus (`tMenu`, its vtable at +0x68): +4 the title's word, +8 the item
//! selected, +0x10 the items (null-terminated, room for 16), +0x50 the screen drawn behind it.
//! A `tOptionsMenu` also has +0x74, how far its transition has faded it (0: shown).
//! An item (`tMenuItem`): +4 its word, +0x14 the menu it goes to (a button), +0x18 its vtable,
//! +0x1c a button's callback or a choice's `tListIterator` (16 bytes: +0 the choices' words,
//! zero-terminated shorts, +4 the address of the byte holding the one chosen, +0xc vtable).
//! Words are ZTEXT's (TextSys_Word): the host adds its own to the file (see [`add_words`]).

use super::*;
use crate::settings::{RESOLUTIONS, Settings, WindowMode};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// The words in every ZTEXT.* (12-byte entries: flags, x, y, offset of the text); the host's
/// come after them.
const ZTEXT_WORDS: u32 = 1457;
/// ZTEXT's "ON", "OFF", the game options' "DISPLAY" and "MEMORY CARD", and the words whose
/// look the host's take: "display" (the Display screen's title), its items' labels (a word's
/// position is where the screen puts the item: the labels' are its rows) and "LOCAL" (one of
/// its choices).
const ON_TEXT: u32 = 103;
const OFF_TEXT: u32 = 104;
const DISPLAY_TEXT: u32 = 455;
const MEMORY_CARD_TEXT: u32 = 458;
const TITLE_TEXT: u32 = 478;
const ROW_TEXTS: [u32; 8] = [480, 482, 483, 484, 485, 486, 487, 488];
const CHOICE_TEXT: u32 = 501;

/// The game options' menu is the one the game setup's third item (Game Options) goes to.
const GAME_OPTIONS_ITEM: u32 = 2;
/// The Display screen's menu (`tOptionsMenu`) and the vtables of its items: a choice between
/// several (left / right, one shown) and between two (both shown, as ON / OFF).
const OPTIONS_MENU_VTABLE: u32 = 0x8001_0F00;
const CHOICE_VTABLE: u32 = 0x8001_0D88;
const TWO_CHOICE_VTABLE: u32 = 0x8001_0D30;
const LIST_ITERATOR_VTABLE: u32 = 0x8001_1370;
const MENU_SIZE: u32 = 0x80;
const CHOICE_SIZE: u32 = 0x2C;
const TWO_CHOICE_SIZE: u32 = 0x30;
/// tFEApplication (its current menu at +4).
const FE_APP: u32 = 0x8005_19A8;
/// A `tOptionsMenu`'s fade. The Display screen's background (tScreen DrawBackground) fades
/// with the Display menu's, wherever the menu drawn is: the graphics screen gives it its own.
const MENU_FADE: u32 = 0x74;

/// The Graphics screen, in the kernel's RAM below the game (unused, as the Quit item's): its
/// menu, items, iterators, the choices' bytes and their words.
const GRAPHICS_RAM: u32 = 0x8000_E000;
const GRAPHICS_MENU: u32 = GRAPHICS_RAM;
const GRAPHICS_ITEMS: u32 = GRAPHICS_RAM + 0x80;
const GRAPHICS_ITERATORS: u32 = GRAPHICS_RAM + 0x200;
const GRAPHICS_VALUES: u32 = GRAPHICS_RAM + 0x280;
const GRAPHICS_LISTS: u32 = GRAPHICS_RAM + 0x300;
const LIST_SIZE: u32 = 0x40;

/// The Graphics screen's items: label and choices (`None`: OFF / ON, shown both at once).
const CHOICES: usize = 6;
fn graphics_items() -> [(&'static str, Option<Vec<String>>); CHOICES] {
    [
        ("DISPLAY MODE", Some(["WINDOWED", "BORDERLESS", "FULLSCREEN"].map(String::from).to_vec())),
        ("RESOLUTION", Some(RESOLUTIONS.iter().map(|(w, h)| format!("{w}X{h}")).collect())),
        ("TEXTURE FILTER", Some(["NEAREST", "BILINEAR"].map(String::from).to_vec())),
        ("CRT FILTER", None),
        ("BLOOM", None),
        ("ANTIALIASING", Some(["OFF", "MSAA 2X", "MSAA 4X", "MSAA 8X"].map(String::from).to_vec())),
    ]
}
const MSAA: [u32; 4] = [1, 2, 4, 8];

/// The words the host adds to ZTEXT (numbered from [`ZTEXT_WORDS`]), each with the word whose
/// look (flags and position) it takes: "GRAPHICS" (Memory Card's), its title, then the items'
/// labels (each in its row) and choices.
fn new_words() -> &'static [(String, u32)] {
    static WORDS: std::sync::OnceLock<Vec<(String, u32)>> = std::sync::OnceLock::new();
    WORDS.get_or_init(|| {
        let mut w = vec![("GRAPHICS".to_string(), MEMORY_CARD_TEXT), ("graphics".to_string(), TITLE_TEXT)];
        for ((label, choices), row) in graphics_items().into_iter().zip(ROW_TEXTS) {
            w.push((label.to_string(), row));
            for c in choices.into_iter().flatten().filter(|c| c != "OFF") {
                w.push((c, CHOICE_TEXT));
            }
        }
        w
    })
}

/// The word for `text` (the host's, or ON / OFF).
fn word(text: &str) -> u32 {
    match text {
        "ON" => ON_TEXT,
        "OFF" => OFF_TEXT,
        _ => ZTEXT_WORDS + new_words().iter().position(|(w, _)| w == text).expect("a host word") as u32,
    }
}

/// The host's words were added to the ZTEXT the game read.
static WORDS_ADDED: AtomicBool = AtomicBool::new(false);

/// Add the host's words to a ZTEXT file: entries after the original's (their texts' offsets
/// moved past the longer table), texts at the end.
pub(super) fn add_words(d: &mut Vec<u8>) {
    let n = ZTEXT_WORDS as usize;
    if d.len() < 12 * n || crate::formats::u32le(d, 8) as usize != 12 * n {
        log::warn!("ZTEXT: not the {n} words expected; no graphics screen");
        return;
    }
    let words = new_words();
    let shift = 12 * words.len() as u32;
    // (the texts' offsets count from the file's start)
    let mut table = Vec::with_capacity(12 * (n + words.len()));
    for i in 0..n {
        let e = &d[12 * i..12 * i + 12];
        table.extend_from_slice(&e[..8]);
        table.extend_from_slice(&(crate::formats::u32le(e, 8) + shift).to_le_bytes());
    }
    let mut texts = d[12 * n..].to_vec();
    let end = (table.len() + 12 * words.len() + texts.len()) as u32;
    let mut at = end;
    for (text, like) in words {
        let like = *like as usize;
        table.extend_from_slice(&d[12 * like..12 * like + 8]);
        table.extend_from_slice(&at.to_le_bytes());
        texts.extend_from_slice(text.as_bytes());
        texts.push(0);
        at += text.len() as u32 + 1;
    }
    table.extend(texts);
    *d = table;
    WORDS_ADDED.store(true, Ordering::Relaxed);
}

/// The graphics choices the app's settings make (packed by [`pack`]), for the screen to show;
/// 0 before the app has said.
static SHOWN: AtomicU64 = AtomicU64::new(0);
/// What the player chose on the screen (packed, [`CHOSEN_NEW`] set until the app takes it).
static CHOSEN: AtomicU64 = AtomicU64::new(0);
const SET: u64 = 1 << 62;
const CHOSEN_NEW: u64 = 1 << 63;
/// The app is closing: save (when autosaving) and quit at the front end's next frame.
static CLOSE: AtomicBool = AtomicBool::new(false);

fn pack(v: [u8; CHOICES]) -> u64 {
    v.iter().enumerate().fold(SET, |a, (i, &b)| a | (b as u64) << (8 * i))
}

fn unpack(p: u64) -> [u8; CHOICES] {
    std::array::from_fn(|i| (p >> (8 * i)) as u8)
}

/// The choices `s` makes: window mode, resolution (the nearest offered), texture filter, CRT
/// filter, bloom, antialiasing.
fn choices(s: &Settings) -> [u8; CHOICES] {
    let (w, h) = s.resolution;
    let res = (0..RESOLUTIONS.len()).min_by_key(|&i| {
        let (rw, rh) = RESOLUTIONS[i];
        (rw as i64 - w as i64).abs() + (rh as i64 - h as i64).abs()
    });
    [
        WindowMode::ALL.iter().position(|&m| m == s.window_mode).unwrap_or(0) as u8,
        res.unwrap_or(0) as u8,
        s.bilinear as u8,
        s.crt as u8,
        s.bloom as u8,
        MSAA.iter().rposition(|&m| m <= s.msaa).unwrap_or(0) as u8,
    ]
}

/// The app's settings, for the graphics screen to show (every frame).
pub fn show_graphics(s: &Settings) {
    SHOWN.store(pack(choices(s)), Ordering::Relaxed);
}

/// What the player has chosen on the graphics screen since last asked, put into `s`: only
/// the choices changed, so a resolution not offered stays until the player picks another.
/// Whether there was anything.
pub fn take_graphics_choice(s: &mut Settings) -> bool {
    let p = CHOSEN.swap(0, Ordering::Relaxed);
    if p & CHOSEN_NEW == 0 {
        return false;
    }
    let (new, old) = (unpack(p), choices(s));
    let changed = |i: usize| new[i] != old[i];
    if changed(0) {
        s.window_mode = WindowMode::ALL.get(new[0] as usize).copied().unwrap_or(WindowMode::Windowed);
    }
    if changed(1) {
        s.resolution = RESOLUTIONS.get(new[1] as usize).copied().unwrap_or(s.resolution);
    }
    if changed(2) {
        s.bilinear = new[2] != 0;
    }
    if changed(3) {
        s.crt = new[3] != 0;
    }
    if changed(4) {
        s.bloom = new[4] != 0;
    }
    if changed(5) {
        s.msaa = MSAA.get(new[5] as usize).copied().unwrap_or(1);
    }
    true
}

/// The app is closing: the game saves (when autosaving) and quits at the front end's next
/// frame.
pub fn request_close() {
    CLOSE.store(true, Ordering::Relaxed);
}


/// GenericMenuSaveGame(slot): what the memory card screen's Save does (the "saving" message,
/// the card written, the message of how it went).
const GENERIC_MENU_SAVE_GAME: u32 = 0x8002_CA20;
/// The cheats used this session (FECheat_IsTheUserACryBabyCheater): the game is not saved.
const CHEATS_USED: u32 = 0x8013_E550;
/// Set when how the save went has been told already: the save (cleared as it starts) then
/// shows no message of it at the end, which waits for a button.
const SAVE_TOLD: u32 = 0x8005_1F00;
/// tDialogMessageString::Draw(dialog), a virtual call: the dialog (+0x84 set by its drawing
/// once it has opened fully, which the save waits for; +0x90 its text).
pub(super) const MESSAGE_DRAW: u32 = 0x8001_976C;
/// tDialogBase::Display / Hide(dialog), which play the front end's sounds of a dialog
/// opening and closing (AudioCmn_PlayFESFX, then AudioCmn_PlayFESFXVol, hooked).
const DIALOG_DISPLAY: u32 = 0x8001_8980;
const DIALOG_HIDE: u32 = 0x8001_8A44;
const PLAY_FE_SFX: u32 = 0x8007_A08C;
pub(super) const PLAY_FE_SFX_VOL: u32 = 0x8007_A0AC;
/// ZTEXT's "HELP", at the front end's bottom left: the autosave notice is in its font and
/// colour, on its line at the bottom right (see [`GameHost::autosave_notice`]).
const HELP_TEXT: u32 = 253;
/// What the notice says (lower case, as the game's words are drawn), and the least time it is
/// shown and each dot's, in frames (a save shows only a few: the game's waits for the card
/// pass at once).
const NOTICE: &[u8] = b"autosaving...";
const NOTICE_FRAMES: u32 = 90;
const DOT_FRAMES: u32 = 20;
/// The save's messages while it runs: "PLEASE DO NOT REMOVE THE MEMORY CARD IN MEMORY CARD
/// SLOT 1 / 2", "SAVING", "SAVING TO MEMORY CARD SLOT 1 / 2".
const SAVING_TEXTS: [u32; 5] = [631, 632, 639, 643, 644];

/// The game options' screen and the saves the host makes.
#[derive(Default)]
pub struct Options {
    /// The game options' menu as patched (to see the player leave it).
    game_options: u32,
    /// The Display screen's menu, whose fade its background takes.
    display: u32,
    /// The front end's menu last frame.
    last_menu: u32,
    /// The graphics screen's choices as last seen (or written) and the app's as last read.
    seen: [u8; CHOICES],
    app: u64,
    /// A save waiting for the front end's next frame.
    save_due: bool,
    /// The game quits once the save made for it is done.
    quit_after_save: bool,
    /// A save the host started is running.
    autosaving: bool,
    /// A race has been run since the last save.
    pub(super) raced: bool,
    /// The front end's "help" as last drawn: its font and place.
    help: Option<(super::overlay::FeFont, i32, i32)>,
    /// The frames the autosave notice has been shown.
    notice_frames: Option<u32>,
}

impl GameHost {
    /// Every front-end frame: the game options as the rebuild has them (the front end builds
    /// its menus again after a race), the graphics screen's choices to or from the app, and a
    /// save due when the player is back at the game setup menu from the game options or after
    /// a race.
    pub(super) fn options_frame(&mut self, c: &mut Cpu) {
        let first = c.m.r32(GAME_SETUP_FIRST_ITEM);
        if !(0x8000_0000..0x8020_0000).contains(&first) {
            return;
        }
        let game_setup = first + 4 * MENU_ITEM_SIZE;
        if c.m.r32(game_setup + 0x68) != MENU_NFS4_VTABLE || c.m.r32(game_setup + 0x10) != first {
            return;
        }
        let item = c.m.r32(game_setup + 0x10 + 4 * GAME_OPTIONS_ITEM);
        let options = c.m.r32(item + 0x14);
        if !(0x8000_0000..0x8020_0000).contains(&options) || c.m.r32(options + 0x68) != MENU_NFS4_VTABLE {
            return;
        }
        if self.patch_game_options(c, options) {
            self.options.game_options = options;
        }
        let app = c.m.r32(FE_APP);
        let menu = if (0x8000_0000..0x8020_0000).contains(&app) { c.m.r32(app + 4) } else { 0 };
        if menu != self.options.last_menu {
            if menu == game_setup && (self.options.last_menu == self.options.game_options || self.options.raced) && crate::settings::get().autosave {
                self.options.save_due = true;
            }
            self.options.last_menu = menu;
        }
        if self.options.game_options == options {
            self.sync_graphics(c);
            // (shown or fading out: the menu stays current until its transition is done)
            if menu == GRAPHICS_MENU && self.options.display != 0 {
                let fade = c.m.r32(GRAPHICS_MENU + MENU_FADE);
                c.m.w32(self.options.display + MENU_FADE, fade);
            }
        }
    }

    /// The game options' Memory Card item becomes Graphics, going to the graphics screen
    /// (built from the Display screen's menu and items). Whether the menu is so (now or from
    /// before).
    fn patch_game_options(&mut self, c: &mut Cpu, options: u32) -> bool {
        let items: Vec<u32> = (0..16).map(|k| c.m.r32(options + 0x10 + 4 * k)).take_while(|&i| i != 0).collect();
        let graphics = word("GRAPHICS");
        if items.iter().any(|&i| c.m.r32(i + 4) == graphics && c.m.r32(i + 0x14) == GRAPHICS_MENU) {
            return true;
        }
        self.options.display = 0;
        let Some(&memory_card) = items.iter().find(|&&i| c.m.r32(i + 4) == MEMORY_CARD_TEXT) else { return false };
        if !WORDS_ADDED.load(Ordering::Relaxed) {
            return false;
        }
        // the Display screen: the menu the item with its word goes to
        let Some(display) = items.iter().find(|&&i| c.m.r32(i + 4) == DISPLAY_TEXT).map(|&i| c.m.r32(i + 0x14)) else { return false };
        if c.m.r32(display + 0x68) != OPTIONS_MENU_VTABLE {
            return false;
        }
        let display_items: Vec<u32> = (0..16).map(|k| c.m.r32(display + 0x10 + 4 * k)).take_while(|&i| i != 0).collect();
        let choice = display_items.iter().copied().find(|&i| c.m.r32(i + 0x18) == CHOICE_VTABLE);
        let two_choice = display_items.iter().copied().find(|&i| c.m.r32(i + 0x18) == TWO_CHOICE_VTABLE);
        let (Some(choice), Some(two_choice)) = (choice, two_choice) else { return false };
        let copy = |c: &mut Cpu, from: u32, to: u32, n: u32| {
            for i in (0..n).step_by(4) {
                let w = c.m.r32(from + i);
                c.m.w32(to + i, w);
            }
        };
        // the menu: the Display one's, with the title and none selected (as built)
        copy(c, display, GRAPHICS_MENU, MENU_SIZE);
        c.m.w16(GRAPHICS_MENU + 4, word("graphics") as u16);
        c.m.w32(GRAPHICS_MENU + 8, 0);
        let start = self.shown();
        for (k, (label, choices)) in graphics_items().into_iter().enumerate() {
            let k = k as u32;
            let (item, iterator, value, list) = (GRAPHICS_ITEMS + 0x30 * k, GRAPHICS_ITERATORS + 0x10 * k, GRAPHICS_VALUES + k, GRAPHICS_LISTS + LIST_SIZE * k);
            let words: Vec<u32> = match &choices {
                Some(choices) => choices.iter().map(|c| word(c)).collect(),
                None => vec![OFF_TEXT, ON_TEXT],
            };
            for (i, &w) in words.iter().chain(&[0]).enumerate() {
                c.m.w16(list + 2 * i as u32, w as u16);
            }
            c.m.w8(value, start[k as usize].min(words.len() as u8 - 1));
            for (o, w) in [list, value, 0, LIST_ITERATOR_VTABLE].into_iter().enumerate() {
                c.m.w32(iterator + 4 * o as u32, w);
            }
            match choices {
                Some(_) => copy(c, choice, item, CHOICE_SIZE),
                None => copy(c, two_choice, item, TWO_CHOICE_SIZE),
            }
            c.m.w32(item + 4, word(label));
            c.m.w32(item + 0x1C, iterator);
            c.m.w32(GRAPHICS_MENU + 0x10 + 4 * k, item);
        }
        c.m.w32(GRAPHICS_MENU + 0x10 + 4 * CHOICES as u32, 0);
        // the Memory Card item, now Graphics
        c.m.w32(memory_card + 4, graphics);
        c.m.w32(memory_card + 0x14, GRAPHICS_MENU);
        c.m.w32(memory_card + 0x1C, 0);
        self.options.display = display;
        self.options.seen = self.graphics_values(c);
        self.options.app = SHOWN.load(Ordering::Relaxed);
        true
    }

    /// The choices the app's settings make (the settings read at start before it has said).
    fn shown(&self) -> [u8; CHOICES] {
        match SHOWN.load(Ordering::Relaxed) {
            0 => choices(crate::settings::get()),
            p => unpack(p),
        }
    }

    fn graphics_values(&self, c: &mut Cpu) -> [u8; CHOICES] {
        std::array::from_fn(|k| c.m.r8(GRAPHICS_VALUES + k as u32))
    }

    /// The player's choices to the app; the app's settings, when they change (the function
    /// keys, the settings file), to the screen.
    fn sync_graphics(&mut self, c: &mut Cpu) {
        let now = self.graphics_values(c);
        let app = SHOWN.load(Ordering::Relaxed);
        if now != self.options.seen {
            CHOSEN.store(pack(now) | CHOSEN_NEW, Ordering::Relaxed);
            self.options.seen = now;
        } else if app != self.options.app && app != 0 {
            let v = unpack(app);
            for (k, &b) in v.iter().enumerate() {
                c.m.w8(GRAPHICS_VALUES + k as u32, b);
            }
            self.options.seen = v;
        }
        self.options.app = app;
    }

    /// The front end's frame is about to run (also within a save's frames): a save due (the
    /// game options left, the app closing) is made first, as the memory card screen's Save
    /// would; quitting, the game quits once its save is done.
    pub(super) fn options_before_frame(&mut self, c: &mut Cpu) {
        if c.stack.contains(&GENERIC_MENU_SAVE_GAME) {
            return;
        }
        if std::mem::take(&mut self.options.autosaving) {
            c.m.w32(SAVE_TOLD, 0);
        }
        if std::mem::take(&mut self.options.quit_after_save) {
            self.quit = true;
            return;
        }
        if CLOSE.swap(false, Ordering::Relaxed) {
            self.quit_with_save(c);
        } else if std::mem::take(&mut self.options.save_due) {
            self.autosave(c);
        }
    }

    /// Quit, after a save when autosaving.
    pub(super) fn quit_with_save(&mut self, c: &mut Cpu) {
        if self.autosave(c) {
            self.options.quit_after_save = true;
        } else {
            self.quit = true;
        }
    }

    /// Save the game (when autosaving and the game saves at all: not after cheats), once the
    /// host call has returned.
    pub(super) fn autosave(&mut self, c: &mut Cpu) -> bool {
        if !crate::settings::get().autosave || c.m.r32(CHEATS_USED) != 0 {
            return false;
        }
        c.deferred.push((GENERIC_MENU_SAVE_GAME, vec![0]));
        self.options.autosaving = true;
        self.options.raced = false;
        true
    }

    /// Font_TextXY in the front end: the "help" at the bottom left (in any language) gives the
    /// autosave notice its font, colour and line.
    pub(super) fn note_fe_text(&mut self, c: &mut Cpu, text: u32, x: i32, y: i32) {
        let ztext = c.m.r32(GP + 0xF00);
        if ztext == 0 {
            return;
        }
        let help = ztext + c.m.r32(ztext + 12 * HELP_TEXT + 8);
        let same = (0..64).map(|i| (c.m.r8(text + i), c.m.r8(help + i))).take_while(|&(a, b)| a != 0 || b != 0).all(|(a, b)| a.eq_ignore_ascii_case(&b));
        if same {
            self.options.help = super::overlay::FeFont::current(c).map(|f| (f, x, y));
        }
    }

    /// While the host saves (and a moment after): "autosaving..." at the bottom right in the
    /// help's font and colour (as far from the right as the help is from the left), its dots
    /// counting.
    pub(super) fn autosave_notice(&mut self, c: &mut Cpu) {
        let shown = match self.options.notice_frames {
            Some(n) if !self.options.autosaving && n >= NOTICE_FRAMES => None,
            Some(n) => Some(n + 1),
            None if self.options.autosaving => Some(0),
            None => None,
        };
        self.options.notice_frames = shown;
        let (Some(shown), Some((font, x, y))) = (shown, self.options.help) else { return };
        let left = self.display[2] as i32 - x - font.width(c, NOTICE);
        let dots = (shown / DOT_FRAMES % 4) as usize;
        // over everything: the front end's commands are drawn last recorded first (as the
        // game's ordering table has them)
        let drawn = std::mem::take(&mut self.overlay);
        self.fe_text(c, &font, &NOTICE[..NOTICE.len() - 3 + dots], left, y);
        let notice = std::mem::replace(&mut self.overlay, drawn);
        if !self.overlay_ot.is_empty() {
            self.overlay_ot.splice(0..0, std::iter::repeat_n(self.current_ot, notice.len()));
        }
        self.overlay.splice(0..0, notice);
    }

    /// A save the host started is running and `dialog` shows one of its messages of it.
    fn saving_message(&self, c: &mut Cpu, dialog: u32) -> bool {
        if !self.options.autosaving || !c.stack.contains(&GENERIC_MENU_SAVE_GAME) {
            return false;
        }
        // TextSys_Word: ZTEXT as loaded (its table's offsets from its start)
        let ztext = c.m.r32(GP + 0xF00);
        let text = c.m.r32(dialog + 0x90);
        SAVING_TEXTS.iter().any(|&w| ztext + c.m.r32(ztext + 12 * w + 8) == text)
    }

    /// A dialog drawn: one of the save's messages while the host saves is not (only marked
    /// open, as its drawing would). Whether it was skipped.
    pub(super) fn skip_saving_message(&mut self, c: &mut Cpu, dialog: u32) -> bool {
        if !self.saving_message(c, dialog) {
            return false;
        }
        c.m.w32(dialog + 0x84, 1);
        true
    }

    /// A front-end sound: not the save's messages opening and closing while the host saves
    /// (Display has the dialog in s1, Hide in s2). Whether it was skipped.
    pub(super) fn skip_saving_sound(&mut self, c: &mut Cpu) -> bool {
        let dialog = if c.stack.ends_with(&[DIALOG_DISPLAY, PLAY_FE_SFX]) {
            c.r[17]
        } else if c.stack.ends_with(&[DIALOG_HIDE, PLAY_FE_SFX]) {
            c.r[18]
        } else {
            return false;
        };
        self.saving_message(c, dialog)
    }

    /// The card has been written: a save the host started went well, so it ends without the
    /// message saying so (which would wait for a button); a failed one tells as the game does.
    pub(super) fn card_written(&mut self, c: &mut Cpu) {
        if self.options.autosaving && c.stack.contains(&GENERIC_MENU_SAVE_GAME) {
            c.m.w32(SAVE_TOLD, 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_round_trip() {
        let mut s = Settings::default();
        s.resolution = (1920, 1080);
        s.msaa = 4;
        let v = choices(&s);
        assert_eq!(unpack(pack(v)), v);
        // the player picks another resolution and borderless; the rest stays
        let mut new = v;
        new[0] = 1;
        new[1] = RESOLUTIONS.iter().position(|&r| r == (2560, 1440)).unwrap() as u8;
        CHOSEN.store(pack(new) | CHOSEN_NEW, Ordering::Relaxed);
        let mut t = s.clone();
        assert!(take_graphics_choice(&mut t));
        assert_eq!((t.window_mode, t.resolution, t.msaa, t.bilinear), (WindowMode::Borderless, (2560, 1440), 4, s.bilinear));
        assert!(!take_graphics_choice(&mut t));
        // a resolution not offered shows as the nearest, and stays unless changed
        s.resolution = (1920, 1079);
        assert_eq!(RESOLUTIONS[choices(&s)[1] as usize], (1920, 1080));
    }

    #[test]
    fn words_added() {
        let mut d = std::fs::read("gamedata/ZTEXT.ENG").unwrap_or_default();
        if d.is_empty() {
            return;
        }
        let orig = d.clone();
        add_words(&mut d);
        let text = |d: &[u8], i: u32| {
            let o = crate::formats::u32le(d, 12 * i as usize + 8) as usize;
            String::from_utf8_lossy(&d[o..o + d[o..].iter().position(|&b| b == 0).unwrap()]).into_owned()
        };
        assert_eq!(text(&d, 92), text(&orig, 92));
        assert_eq!(text(&d, ZTEXT_WORDS - 1), text(&orig, ZTEXT_WORDS - 1));
        assert_eq!(text(&d, word("GRAPHICS")), "GRAPHICS");
        assert_eq!(text(&d, word("2560X1440")), "2560X1440");
        // the labels in the Display screen's rows
        assert_eq!(d[12 * word("RESOLUTION") as usize + 6], orig[12 * 482 + 6]);
    }
}
