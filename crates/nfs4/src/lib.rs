//! NFS4.EXE decompiled to Rust: the original's functions rewritten as readable code, one at a
//! time, each producing exactly the recompiled original's results (`crates/nfs4rc`).
//!
//! The game's state stays in PS1 RAM, where the code not yet decompiled reads it; the
//! decompiled functions reach it through the typed views in [`types`]. The recompiled calls to
//! a decompiled function ask [`install`]'s hook first (tools/decomp_funcs.txt lists them), and
//! the recompiled original stays the fallback and the reference [`check`] compares against.
//!
//! Modules are named after the original's source files (PHYSICS.CPP is [`physics`]...).
//!
//! Switches (environment, read once):
//! - `NFSHS_DECOMP=off`: run the recompiled originals only.
//! - `NFSHS_DECOMP_OFF=Name,...`: those functions run recompiled (for bisecting).
//! - `NFSHS_DECOMP_ONLY=Name,...`: only those run decompiled. Decompiled functions call each
//!   other directly, so this is how a function called from decompiled code gets checked on
//!   its own: its callers then run recompiled and call it through the hook.
//! - `NFSHS_DECOMP_CHECK=all|Name,...[,abort]`: shadow checks (see [`check`]).
//! - `NFSHS_DECOMP_STATS=1`: calls and checks per function when a game thread ends.

pub mod abi;
pub mod ai;
pub mod aicop;
pub mod aidatarecord;
pub mod aidelaycar;
pub mod aih_basiccop;
pub mod aih_basicperp;
pub mod aih_btccop;
pub mod aih_btcperp;
pub mod aih_cop;
pub mod aih_hum;
pub mod aih_opp;
pub mod aih_play;
pub mod aih_traf;
pub mod aihigh;
pub mod aiinit;
pub mod ailife;
pub mod aiperson;
pub mod aiphysic;
pub mod aiscript;
pub mod aispeeds;
pub mod aistate;
pub mod aitrigger;
pub mod aitune;
pub mod aiworld;
pub mod bworld;
pub mod bworldsm;
pub mod cars;
pub mod call;
pub mod camera;
pub mod check;
pub mod collide;
pub mod control;
pub mod drawshp;
pub mod feapp;
pub mod feaudio;
pub mod fecars;
pub mod fecheats;
pub mod fecredits;
pub mod fedialog;
pub mod fefades;
pub mod feinput;
pub mod felines;
pub mod fememcard;
pub mod femenu;
pub mod femenudefs;
pub mod fe3dmenu;
pub mod minfront;
pub mod femenuextended;
pub mod femenuoptions;
pub mod femission;
pub mod fescreen;
pub mod fetextrender;
pub mod fetexture;
pub mod fetools;
pub mod fetourn;
pub mod fetracks;
pub mod fetv;
pub mod fevideowall;
pub mod fixed;
pub mod front;
pub mod globals;
pub mod gpu;
pub mod hud;
pub mod mathnfs;
pub mod memcard;
pub mod mmeffect;
pub mod mpause;
pub mod pausemenu;
pub mod movie;
pub mod newton;
pub mod night;
pub mod physics;
pub mod psxfront;
pub mod replay;
pub mod rpause;
pub mod schedule;
pub mod skidmark;
pub mod screenaudio;
pub mod screencarselect;
pub mod screencongrats;
pub mod screencontroller;
pub mod screendisplay;
pub mod screenmain;
pub mod screenmemcard;
pub mod screenpinkslips;
pub mod screenpost;
pub mod screentrackinfo;
pub mod screentournselect;
pub mod screentrackrecords;
pub mod screentracks;
pub mod screentrophyinfo;
pub mod screentrophyroom;
pub mod screenusername;
pub mod sim;
pub mod simqueue;
pub mod statchk;
pub mod stats;
pub mod stattool;
pub mod texture;
pub mod textureprocess;
pub mod trsproj;
pub mod types;
pub mod udff;
pub mod vec;
pub mod weather;

use ps1rt::Cpu;
use std::cell::{Cell, RefCell};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// A decompiled function: its retail address and name, and its entry with the original's
/// calling convention (arguments in a0..a3 and on the stack, result in v0).
pub struct Func {
    pub addr: u32,
    pub name: &'static str,
    pub run: fn(&mut Cpu),
    /// It returns a value (v0 is compared by the checks).
    pub returns: bool,
}

const fn func(addr: u32, name: &'static str, run: fn(&mut Cpu)) -> Func {
    Func { addr, name, run, returns: false }
}

const fn func_ret(addr: u32, name: &'static str, run: fn(&mut Cpu)) -> Func {
    Func { addr, name, run, returns: true }
}

/// Every decompiled function the game is routed to, by address.
pub static FUNCS: &[Func] = &[
    func_ret(0x8001_28a0, "MinFront_ParseOptions", minfront::abi_parse_options),
    func(0x8001_28e8, "Fe3D_InitShowroom", fe3dmenu::abi_init_showroom),
    func(0x8001_2a94, "CameraLookAt", fe3dmenu::abi_camera_look_at),
    func(0x8001_2ba8, "Draw_MenuRenderingView", fe3dmenu::abi_draw_menu_rendering_view),
    func(0x8001_2f50, "Draw_EmptyView", fe3dmenu::abi_draw_empty_view),
    func_ret(0x8001_30f8, "tFEApplication::tFEApplication", feapp::abi_fe_application),
    func(0x8001_331c, "tFEApplication::~tFEApplication", feapp::abi_fe_application_dtor),
    func(0x8001_338c, "tFEApplication::PerformMenuInitialization", feapp::abi_perform_menu_initialization),
    func(0x8001_33f4, "tFEApplication::PerformMenuDestruction", feapp::abi_perform_menu_destruction),
    func(0x8001_349c, "tFEApplication::DrawHelpIcons", feapp::abi_draw_help_icons),
    func(0x8001_3894, "tFEApplication::Redraw", feapp::abi_redraw),
    func(0x8001_3e34, "tFEApplication::UpdateMusic", feapp::abi_update_music),
    func(0x8001_3ef4, "tFEApplication::SetMenu", feapp::abi_set_menu),
    func(0x8001_3ff8, "tFEApplication::SetScreen", feapp::abi_set_screen),
    func(0x8001_4048, "tFEApplication::DisplayHelp", feapp::abi_display_help),
    func(0x8001_406c, "FreeHelpShapeCluts", feapp::abi_free_help_shape_cluts),
    func(0x8001_40e8, "tFEApplication::RunDemoVideo", feapp::abi_run_demo_video),
    func_ret(0x8001_43c8, "tFEApplication::MainLoop", feapp::abi_main_loop),
    func_ret(0x8001_5554, "tFEApplication::RunPostGame", feapp::abi_run_post_game),
    func(0x8001_585c, "tFEApplication::RunFrontEnd", feapp::abi_run_front_end),
    func(0x8001_5884, "tDialogMessageStringWithTimeout::~tDialogMessageStringWithTimeout", fedialog::abi_message_with_timeout_destructor),
    func_ret(0x8001_58a8, "FEAudio_StartLoadPatch", feaudio::abi_start_load_patch),
    func(0x8001_594c, "Feaudio_StartPatch", feaudio::abi_start_patch),
    func(0x8001_5a70, "FeAudio_systemtask", feaudio::abi_systemtask),
    func_ret(0x8001_5cf4, "FeAudio_AsyncPlayCommentary", feaudio::abi_async_play_commentary),
    func_ret(0x8001_5de0, "FeAudio_AsyncPlaySpeech", feaudio::abi_async_play_speech),
    func_ret(0x8001_5e5c, "FeAudio_StartBigfileRead", feaudio::abi_start_bigfile_read),
    func(0x8001_5ef4, "FeAudio_LocateBigfile", feaudio::abi_locate_bigfile),
    func_ret(0x8001_604c, "FeAudio_InitViv", feaudio::abi_init_viv),
    func(0x8001_6200, "FeAudio_InitCommentary", feaudio::abi_init_commentary),
    func(0x8001_62a0, "FeAudio_DeInitCommentary", feaudio::abi_deinit_commentary),
    func(0x8001_62d8, "tCarManager::Initialize", fecars::abi_initialize),
    func_ret(0x8001_6374, "tCarManager::GetCarFromID", fecars::abi_get_car_from_id),
    func_ret(0x8001_63e4, "tCarManager::GetCarFromSimID", fecars::abi_get_car_from_sim_id),
    func_ret(0x8001_6434, "tCarManager::CheapestCarStockPrice", fecars::abi_cheapest_car_stock_price),
    func_ret(0x8001_648c, "tCarManager::CalcUsedPrice", fecars::abi_calc_used_price),
    func_ret(0x8001_6598, "tCarManager::PurchaseCar", fecars::abi_purchase_car),
    func_ret(0x8001_6674, "tCarManager::SellCar", fecars::abi_sell_car),
    func_ret(0x8001_67f4, "tCarManager::PurchaseUpgrade", fecars::abi_purchase_upgrade),
    func(0x8001_6900, "tCarManager::RemoveFromPinkSlipsList", fecars::abi_remove_from_pink_slips_list),
    func(0x8001_6a48, "tCarManager::AddToPinkSlipsList", fecars::abi_add_to_pink_slips_list),
    func(0x8001_6ac4, "tCarManager::AddUpgradesToPinkSlipsList", fecars::abi_add_upgrades_to_pink_slips_list),
    func(0x8001_6b9c, "tCarManager::LoadCars", fecars::abi_load_cars),
    func(0x8001_6c38, "tCarManager::SaveCars", fecars::abi_save_cars),
    func(0x8001_6cb0, "tCarManager::LoadPinkSlipsCars", fecars::abi_load_pink_slips_cars),
    func(0x8001_6d54, "tCarManager::SavePinkSlipsCars", fecars::abi_save_pink_slips_cars),
    func(0x8001_6eb0, "tCarManager::SetClassAvailable", fecars::abi_set_class_available),
    func(0x8001_6f34, "tCarManager::SetCarAvailable", fecars::abi_set_car_available),
    func(0x8001_6f50, "tCarManager::SetClassViewable", fecars::abi_set_class_viewable),
    func(0x8001_6fbc, "tCarManager::SetCarViewable", fecars::abi_set_car_viewable),
    func(0x8001_6fd8, "tCarManager::GetStockCar", fecars::abi_get_stock_car),
    func(0x8001_7094, "tCarManager::GetGarageCar", fecars::abi_get_garage_car),
    func(0x8001_71a0, "tCarManager::GetPinkSlipsCar", fecars::abi_get_pink_slips_car),
    func(0x8001_727c, "tCarManager::LoadDescription", fecars::abi_load_description),
    func(0x8001_743c, "tCarManager::ReleaseDescription", fecars::abi_release_description),
    func_ret(0x8001_747c, "tCarManager::GetNumOwnedCars", fecars::abi_get_num_owned_cars),
    func_ret(0x8001_74c0, "tCarManager::GetNumTourneyCars", fecars::abi_get_num_tourney_cars),
    func_ret(0x8001_7568, "tCarManager::GetNumPinkSlipsCars", fecars::abi_get_num_pink_slips_cars),
    func_ret(0x8001_75ac, "tCarManager::GetClassList", fecars::abi_get_class_list),
    func(0x8001_76c0, "tCarManager::InitializeIngameCarList", fecars::abi_initialize_ingame_car_list),
    func_ret(0x8001_7740, "tCarManager::IsCarAnAddedModel", fecars::abi_is_car_an_added_model),
    func(0x8001_77c0, "tCarManager::AddCarToIngameList", fecars::abi_add_car_to_ingame_list),
    func_ret(0x8001_7860, "tCarManager::FindSimilarCar", fecars::abi_find_similar_car),
    func_ret(0x8001_7a14, "tListIteratorCar::tListIteratorCar", fecars::abi_list_iterator_car),
    func(0x8001_7a70, "tListIteratorCar::~tListIteratorCar", fecars::abi_list_iterator_car_dtor),
    func_ret(0x8001_7a98, "tListIteratorCar::Value", fecars::abi_list_iterator_car_value),
    func_ret(0x8001_7ac4, "tListIteratorCar::TextValue", fecars::abi_list_iterator_car_text_value),
    func(0x8001_7be4, "tListIteratorCar::AdjustPosition", fecars::abi_adjust_position),
    func(0x8001_81d4, "tListIteratorCar::Increment", fecars::abi_list_iterator_car_increment),
    func(0x8001_81f4, "tListIteratorCar::Decrement", fecars::abi_list_iterator_car_decrement),
    func_ret(0x8001_8214, "tListIteratorCar::ValidCar", fecars::abi_valid_car),
    func_ret(0x8001_85e0, "tListIteratorCarColor::tListIteratorCarColor", fecars::abi_list_iterator_car_color),
    func(0x8001_8660, "tListIteratorCarColor::~tListIteratorCarColor", fecars::abi_list_iterator_car_color_dtor),
    func_ret(0x8001_8688, "tListIteratorCarColor::Value", fecars::abi_list_iterator_car_color_value),
    func_ret(0x8001_86e8, "tListIteratorCarColor::TextValue", fecars::abi_list_iterator_car_color_text_value),
    func(0x8001_86f0, "tListIteratorCarColor::Increment", fecars::abi_list_iterator_car_color_increment),
    func(0x8001_8788, "tListIteratorCarColor::Decrement", fecars::abi_list_iterator_car_color_decrement),
    func_ret(0x8001_8808, "tDialogBase::ShouldTimeOut", fedialog::abi_should_time_out),
    func(0x8001_8840, "tDialogBase::InitializeClass", fedialog::abi_initialize_class),
    func(0x8001_8860, "tDialogBase::DrawAllDialogs", fedialog::abi_draw_all_dialogs),
    func(0x8001_8930, "tDialogBase::HideAllDialogs", fedialog::abi_hide_all_dialogs),
    func_ret(0x8001_8970, "tDialogBase::GetTopMostDialog", fedialog::abi_get_top_most_dialog),
    func(0x8001_8980, "tDialogBase::Display", fedialog::abi_display),
    func(0x8001_8a44, "tDialogBase::Hide", fedialog::abi_hide),
    func(0x8001_8af8, "tDialogBase::Draw", fedialog::abi_draw),
    func(0x8001_8c08, "tDialogBase::ProcessInput", fedialog::abi_process_input),
    func(0x8001_8c4c, "tDialogHelp::AddItem", fedialog::abi_add_item),
    func(0x8001_8cd0, "tDialogHelp::CalculateDimensions", fedialog::abi_calculate_dimensions_help),
    func(0x8001_926c, "tDialogHelp::Draw", fedialog::abi_draw_help),
    func(0x8001_9558, "tDialogMessageString::CalculateDimensions", fedialog::abi_calculate_dimensions_message),
    func(0x8001_976c, "tDialogMessageString::Draw", fedialog::abi_draw_message),
    func(0x8001_98d0, "tDialogBackUpOnly::ProcessInput", fedialog::abi_process_input_back_up_only),
    func_ret(0x8001_9928, "tDialogInteractive::Run", fedialog::abi_run),
    func(0x8001_9b14, "tDialogYesNo::CalculateDimensions", fedialog::abi_calculate_dimensions_yes_no),
    func_ret(0x8001_9bcc, "tDialogYesNo::tDialogYesNo", fedialog::abi_yes_no),
    func(0x8001_9c84, "tDialogYesNo::Draw", fedialog::abi_draw_yes_no),
    func(0x8001_9e0c, "tDialogYesNo::ProcessInput", fedialog::abi_process_input_yes_no),
    func(0x8001_9ed8, "tDialogYesNoMem::ProcessInput", fedialog::abi_process_input_yes_no_mem),
    func(0x8001_9fbc, "tDialogYesNoTri::ProcessInput", fedialog::abi_process_input_yes_no_tri),
    func(0x8001_9ffc, "tDialogNoInputMessage::ProcessInput", fedialog::abi_process_input_no_input),
    func(0x8001_a004, "tDialogYesNoTri::~tDialogYesNoTri", fedialog::abi_yes_no_tri_destructor),
    func(0x8001_a024, "tDialogYesNoMem::~tDialogYesNoMem", fedialog::abi_yes_no_mem_destructor),
    func(0x8001_a044, "tDialogYesNo::~tDialogYesNo", fedialog::abi_yes_no_destructor),
    func(0x8001_a064, "tDialogInteractive::~tDialogInteractive", fedialog::abi_interactive_destructor),
    func(0x8001_a084, "tDialogNoInputMessage::~tDialogNoInputMessage", fedialog::abi_no_input_destructor),
    func(0x8001_a0a4, "tDialogBackUpOnly::~tDialogBackUpOnly", fedialog::abi_back_up_only_destructor),
    func(0x8001_a0c4, "tDialogMessageString::~tDialogMessageString", fedialog::abi_message_destructor),
    func(0x8001_a0e4, "tDialogHelp::~tDialogHelp", fedialog::abi_help_destructor),
    func(0x8001_a104, "tDialogBase::~tDialogBase", fedialog::abi_base_destructor),
    func(0x8001_a128, "MenuNFS4_SetHelpPos", femenuextended::abi_set_help_pos),
    func(0x8001_a158, "MenuNFS4_DrawTextBox", femenuextended::abi_draw_text_box),
    func_ret(0x8001_a60c, "tMenuItemGoToMenuNFS4Button::tMenuItemGoToMenuNFS4Button", femenuextended::abi_go_to_menu_nfs4_button),
    func(0x8001_a674, "tMenuItemGoToMenuNFS4Button::~tMenuItemGoToMenuNFS4Button", femenuextended::abi_go_to_menu_nfs4_button_dtor),
    func(0x8001_a69c, "tMenuItemGoToMenuNFS4Button::Draw", femenuextended::abi_go_to_menu_nfs4_button_draw),
    func(0x8001_a918, "tMenuItemGoToMenuNFS4Button::TransitionOn", femenuextended::abi_button_transition_on),
    func(0x8001_a928, "tMenuItemGoToMenuNFS4Button::TransitionOff", femenuextended::abi_button_transition_off),
    func_ret(0x8001_a934, "tMenuItemGoToMenuNFS4Button::TransitionIsFinished", femenuextended::abi_button_transition_is_finished),
    func(0x8001_a940, "tMenuItemGoToMenuNFS4Button::UpdateTransition", femenuextended::abi_button_update_transition),
    func_ret(0x8001_a99c, "tMenuItemNFS4LeftRightChoice::tMenuItemNFS4LeftRightChoice", femenuextended::abi_nfs4_left_right_choice),
    func(0x8001_aa00, "tMenuItemNFS4LeftRightChoice::~tMenuItemNFS4LeftRightChoice", femenuextended::abi_nfs4_left_right_choice_dtor),
    func(0x8001_aa28, "tMenuItemNFS4LeftRightChoice::Draw", femenuextended::abi_nfs4_left_right_choice_draw),
    func(0x8001_acbc, "tMenuItemNFS4LeftRightChoice::TransitionOn", femenuextended::abi_choice_transition_on),
    func(0x8001_accc, "tMenuItemNFS4LeftRightChoice::TransitionOff", femenuextended::abi_choice_transition_off),
    func_ret(0x8001_acd8, "tMenuItemNFS4LeftRightChoice::TransitionIsFinished", femenuextended::abi_choice_transition_is_finished),
    func(0x8001_ace4, "tMenuItemNFS4LeftRightChoice::UpdateTransition", femenuextended::abi_choice_update_transition),
    func(0x8001_ad40, "tMenuItemOptionsLeftRightChoice::Draw", femenuextended::abi_options_left_right_choice_draw),
    func(0x8001_af18, "tMenuItemOptionsTwoItemChoice::TransitionOn", femenuextended::abi_two_item_choice_transition_on),
    func(0x8001_af74, "tMenuItemOptionsTwoItemChoice::Draw", femenuextended::abi_two_item_choice_draw),
    func_ret(0x8001_b1f8, "tMenuNFS4::tMenuNFS4", femenuextended::abi_menu_nfs4),
    func_ret(0x8001_b260, "tMenuNFS4::tMenuNFS4(bare)", femenuextended::abi_menu_nfs4_bare),
    func(0x8001_b2b8, "tMenuNFS4::~tMenuNFS4", femenuextended::abi_menu_nfs4_dtor),
    func(0x8001_b2e0, "tMenuNFS4::Initialize", femenuextended::abi_menu_nfs4_initialize),
    func(0x8001_b390, "tMenuNFS4::ProcessInput", femenuextended::abi_menu_nfs4_process_input),
    func(0x8001_b3b0, "tMenuNFS4::TransitionOff", femenuextended::abi_menu_nfs4_transition_off),
    func(0x8001_b438, "tMenuNFS4::TransitionOn", femenuextended::abi_menu_nfs4_transition_on),
    func_ret(0x8001_b4c0, "tMenuNFS4::TransitionIsFinished", femenuextended::abi_menu_nfs4_transition_is_finished),
    func(0x8001_b560, "tMenuNFS4::UpdateTransition", femenuextended::abi_menu_nfs4_update_transition),
    func(0x8001_b5f4, "tMenuNFS4::DrawItem", femenuextended::abi_menu_nfs4_draw_item),
    func(0x8001_b654, "tMenuNFS4::Draw", femenuextended::abi_menu_nfs4_draw),
    func_ret(0x8001_b79c, "tMenuNFS4TwoPlayer::tMenuNFS4TwoPlayer", femenuextended::abi_menu_two_player),
    func(0x8001_b808, "tMenuNFS4TwoPlayer::~tMenuNFS4TwoPlayer", femenuextended::abi_menu_two_player_dtor),
    func(0x8001_b830, "tMenuNFS4TwoPlayer::DrawItem", femenuextended::abi_menu_two_player_draw_item),
    func_ret(0x8001_b8a8, "tMenuNFS4Bottom::tMenuNFS4Bottom", femenuextended::abi_menu_bottom),
    func(0x8001_b910, "tMenuNFS4Bottom::~tMenuNFS4Bottom", femenuextended::abi_menu_bottom_dtor),
    func(0x8001_b938, "tMenuNFS4Bottom::Draw", femenuextended::abi_menu_bottom_draw),
    func_ret(0x8001_b9f8, "tMenuBlank::tMenuBlank", femenuextended::abi_menu_blank),
    func(0x8001_ba5c, "tMenuBlank::~tMenuBlank", femenuextended::abi_menu_blank_dtor),
    func(0x8001_ba84, "tMenuBlank::Draw", femenuextended::abi_menu_blank_draw),
    func(0x8001_bae8, "tMenuBlank::ProcessInput", femenuextended::abi_menu_blank_process_input),
    func(0x8001_bb2c, "tMenuBlank::Initialize", femenuextended::abi_nothing),
    func_ret(0x8001_bb34, "tMenuBlank::DebounceKeys", femenuextended::abi_menu_blank_debounce_keys),
    func(0x8001_bb3c, "tMenuBlank::TransitionOff", femenuextended::abi_menu_blank_transition_off),
    func(0x8001_bb58, "tMenuBlank::TransitionOn", femenuextended::abi_menu_blank_transition_on),
    func_ret(0x8001_bb70, "tMenuBlank::TransitionIsFinished", femenuextended::abi_menu_blank_transition_is_finished),
    func_ret(0x8001_bbac, "tMenuOptions::tMenuOptions", femenuextended::abi_menu_options),
    func(0x8001_bc24, "tMenuOptions::~tMenuOptions", femenuextended::abi_menu_options_dtor),
    func(0x8001_bc4c, "tMenuOptions::Draw", femenuextended::abi_menu_options_draw),
    func(0x8001_bf24, "tMenuOptions::TransitionOff", femenuextended::abi_menu_options_transition_off),
    func(0x8001_bf60, "tMenuOptions::TransitionOn", femenuextended::abi_menu_options_transition_on),
    func_ret(0x8001_bff8, "tMenuOptions::TransitionIsFinished", femenuextended::abi_menu_options_transition_is_finished),
    func(0x8001_c01c, "tMenuOptions::ProcessInput", femenuextended::abi_menu_options_process_input),
    func_ret(0x8001_c050, "tMenuOptions::IsSubMenu", femenuextended::abi_menu_options_is_sub_menu),
    func(0x8001_c058, "tMenuBlank::UpdateTransition", femenuextended::abi_nothing),
    func(0x8001_c060, "tMenuItemOptionsTwoItemChoice::~tMenuItemOptionsTwoItemChoice", femenuextended::abi_two_item_choice_dtor),
    func(0x8001_c080, "tMenuItemOptionsLeftRightChoice::~tMenuItemOptionsLeftRightChoice", femenuextended::abi_options_left_right_choice_dtor),
    func(0x8001_c0a0, "tMenuItemGoToMenuNFS4Button::Draw(bool)", femenuextended::abi_nothing),
    func(0x8001_c0a8, "CalcPulsateYellow", femenuoptions::abi_calc_pulsate_yellow),
    func(0x8001_c100, "DrawLeftFlare", femenuoptions::abi_draw_left_flare),
    func(0x8001_c3bc, "tMenuItemGoToMenuButtonFade::TransitionOff", femenuoptions::abi_button_fade_transition_off),
    func(0x8001_c3c8, "tMenuItemGoToMenuButtonFade::TransitionOn", femenuoptions::abi_button_fade_transition_on),
    func_ret(0x8001_c3e4, "tMenuItemGoToMenuButtonFade::TransitionIsFinished", femenuoptions::abi_button_fade_transition_is_finished),
    func(0x8001_c43c, "tMenuItemGoToMenuButtonFade::UpdateTransition", femenuoptions::abi_button_fade_update_transition),
    func_ret(0x8001_c4b8, "tMenuItemLeftRightFade::tMenuItemLeftRightFade", femenuoptions::abi_left_right_fade),
    func(0x8001_c4f4, "tMenuItemLeftRightFade::TransitionOff", femenuoptions::abi_left_right_fade_transition_off),
    func(0x8001_c50c, "tMenuItemLeftRightFade::TransitionOn", femenuoptions::abi_left_right_fade_transition_on),
    func_ret(0x8001_c530, "tMenuItemLeftRightFade::TransitionIsFinished", femenuoptions::abi_left_right_fade_transition_is_finished),
    func(0x8001_c598, "tMenuItemLeftRightFade::UpdateTransition", femenuoptions::abi_left_right_fade_update_transition),
    func_ret(0x8001_c614, "tOptionsMenu::tOptionsMenu", femenuoptions::abi_options_menu),
    func(0x8001_c6a4, "tOptionsMenu::~tOptionsMenu", femenuoptions::abi_options_menu_dtor),
    func_ret(0x8001_c6cc, "tOptionsMenu::DebounceKeys", femenuoptions::abi_options_menu_debounce_keys),
    func(0x8001_c738, "tOptionsMenu::TransitionOff", femenuoptions::abi_options_menu_transition_off),
    func(0x8001_c7d0, "tOptionsMenu::TransitionOn", femenuoptions::abi_options_menu_transition_on),
    func_ret(0x8001_c86c, "tOptionsMenu::TransitionIsFinished", femenuoptions::abi_options_menu_transition_is_finished),
    func(0x8001_c9b4, "tOptionsMenu::UpdateTransition", femenuoptions::abi_options_menu_update_transition),
    func(0x8001_cc64, "tOptionsMenu::Draw", femenuoptions::abi_options_menu_draw),
    func(0x8001_ce68, "tOptionsMenu::ProcessInput", femenuoptions::abi_options_menu_process_input),
    func_ret(0x8001_cef0, "tInsideBoxMenu::tInsideBoxMenu", femenuoptions::abi_inside_box_menu),
    func(0x8001_cf64, "tInsideBoxMenu::~tInsideBoxMenu", femenuoptions::abi_inside_box_menu_dtor),
    func(0x8001_cf8c, "tInsideBoxMenu::ProcessInput", femenuoptions::abi_inside_box_menu_process_input),
    func(0x8001_d05c, "tInsideBoxMenu::Draw", femenuoptions::abi_inside_box_menu_draw),
    func_ret(0x8001_d2c0, "tMenuItemSlidingMenu::tMenuItemSlidingMenu", femenuoptions::abi_sliding_menu),
    func(0x8001_d368, "tMenuItemSlidingMenu::~tMenuItemSlidingMenu", femenuoptions::abi_sliding_menu_dtor),
    func(0x8001_d390, "tMenuItemSlidingMenu::TransitionOff", femenuoptions::abi_sliding_menu_transition_off),
    func(0x8001_d3bc, "tMenuItemSlidingMenu::TransitionOn", femenuoptions::abi_sliding_menu_transition_on),
    func_ret(0x8001_d3f4, "tMenuItemSlidingMenu::TransitionIsFinished", femenuoptions::abi_sliding_menu_transition_is_finished),
    func(0x8001_d47c, "tMenuItemSlidingMenu::UpdateTransition", femenuoptions::abi_sliding_menu_update_transition),
    func_ret(0x8001_d4f8, "tMenuItemSlidingMenu::DebounceKeys", femenuoptions::abi_sliding_menu_debounce_keys),
    func(0x8001_d540, "tMenuItemSlidingMenu::Draw(bool)", femenuoptions::abi_sliding_menu_draw_bool),
    func(0x8001_d57c, "tMenuItemSlidingMenu::UpdatefOpenHeight", femenuoptions::abi_sliding_menu_update_open_height),
    func(0x8001_d79c, "tMenuItemSlidingMenu::Draw", femenuoptions::abi_sliding_menu_draw),
    func(0x8001_dde0, "tMenuItemSlidingMenu::ProcessInput", femenuoptions::abi_sliding_menu_process_input),
    func(0x8001_de88, "tMenuItemSlidingMenu::SetMenu", femenuoptions::abi_sliding_menu_set_menu),
    func(0x8001_decc, "tMenuItemSlidingActivated::UpdatefOpenHeight", femenuoptions::abi_sliding_activated_update_open_height),
    func(0x8001_df34, "tMenuItemSlidingActivated::TransitionOff", femenuoptions::abi_sliding_menu_transition_off),
    func(0x8001_df60, "tMenuItemSlidingActivated::TransitionOn", femenuoptions::abi_sliding_activated_transition_on),
    func_ret(0x8001_dfa0, "tMenuItemSlidingActivated::TransitionIsFinished", femenuoptions::abi_sliding_menu_transition_is_finished),
    func(0x8001_e028, "tMenuItemSlidingActivated::UpdateTransition", femenuoptions::abi_sliding_activated_update_transition),
    func(0x8001_e0a4, "tMenuItemSlidingActivated::ProcessInput", femenuoptions::abi_sliding_activated_process_input),
    func(0x8001_e29c, "tMenuItemLeftRightFade::MyLeftRightDraw", femenuoptions::abi_my_left_right_draw),
    func(0x8001_e390, "tMenuItemDisplayLeftRightChoice::Draw", femenuoptions::abi_display_choice_draw),
    func(0x8001_e480, "tMenuItemOnOffLeftRightChoice::TransitionOn", femenuoptions::abi_on_off_transition_on),
    func(0x8001_e4e4, "tMenuItemOnOffLeftRightChoice::Draw", femenuoptions::abi_on_off_draw),
    func_ret(0x8001_e65c, "tMenuItemLeftRightAudioSlider::tMenuItemLeftRightAudioSlider", femenuoptions::abi_audio_slider),
    func(0x8001_e6a8, "tMenuItemLeftRightAudioSlider::~tMenuItemLeftRightAudioSlider", femenuoptions::abi_audio_slider_dtor),
    func(0x8001_e6d0, "tMenuItemLeftRightAudioSlider::Draw", femenuoptions::abi_audio_slider_draw),
    func_ret(0x8001_e8dc, "tMenuItemLeftRightAudioSlider::Percentage", femenuoptions::abi_audio_slider_percentage),
    func(0x8001_e9b0, "tMenuItemLeftRightAudioSlider::TransitionOff", femenuoptions::abi_audio_slider_transition_off),
    func(0x8001_e9c0, "tMenuItemLeftRightAudioSlider::TransitionOn", femenuoptions::abi_audio_slider_transition_on),
    func_ret(0x8001_e9d8, "tMenuItemLeftRightAudioSlider::TransitionIsFinished", femenuoptions::abi_audio_slider_transition_is_finished),
    func(0x8001_ea34, "tMenuItemLeftRightAudioSlider::UpdateTransition", femenuoptions::abi_audio_slider_update_transition),
    func_ret(0x8001_ea80, "tInsideBoxSongMenu::tInsideBoxSongMenu", femenuoptions::abi_song_menu),
    func(0x8001_eb10, "tInsideBoxSongMenu::~tInsideBoxSongMenu", femenuoptions::abi_song_menu_dtor),
    func(0x8001_eb38, "tInsideBoxSongMenu::Draw", femenuoptions::abi_song_menu_draw),
    func(0x8001_eda8, "tInsideBoxSongMenu::DrawOneSong", femenuoptions::abi_song_menu_draw_one_song),
    func_ret(0x8001_efd4, "tInsideBoxSongMenu::DebounceKeys", femenuoptions::abi_song_menu_debounce_keys),
    func(0x8001_efdc, "tInsideBoxSongMenu::ProcessInput", femenuoptions::abi_song_menu_process_input),
    func(0x8001_f1e4, "tMenuItemControllerLeftRightChoice::Draw", femenuoptions::abi_controller_choice_draw),
    func_ret(0x8001_f3e8, "tInsideBoxLeftRightSlider::tInsideBoxLeftRightSlider", femenuoptions::abi_inside_slider),
    func(0x8001_f428, "tInsideBoxLeftRightSlider::~tInsideBoxLeftRightSlider", femenuoptions::abi_inside_slider_dtor),
    func(0x8001_f450, "tInsideBoxLeftRightSlider::Draw", femenuoptions::abi_inside_slider_draw),
    func_ret(0x8001_f5d4, "tInsideBoxTwoWaySlider::tInsideBoxTwoWaySlider", femenuoptions::abi_two_way_slider),
    func(0x8001_f628, "tInsideBoxTwoWaySlider::~tInsideBoxTwoWaySlider", femenuoptions::abi_two_way_slider_dtor),
    func(0x8001_f650, "tInsideBoxTwoWaySlider::ProcessInput", femenuoptions::abi_two_way_slider_process_input),
    func(0x8001_f700, "tInsideBoxTwoWaySlider::Draw", femenuoptions::abi_two_way_slider_draw),
    func_ret(0x8001_f9c0, "GetCurrentStickRange", femenuoptions::abi_get_current_stick_range),
    func_ret(0x8001_fa1c, "GetCurrentStickRange2", femenuoptions::abi_get_current_stick_range2),
    func(0x8001_fa78, "tInsideBoxTwoWaySlider::Calibrate", femenuoptions::abi_calibrate),
    func_ret(0x8001_fd98, "tUserNameMenuItem::tUserNameMenuItem", femenuoptions::abi_user_name),
    func_ret(0x8001_fe0c, "CheckForCheats", femenuoptions::abi_check_for_cheats),
    func(0x8001_fe8c, "tUserNameMenuItem::ProcessInput", femenuoptions::abi_user_name_process_input),
    func_ret(0x8002_024c, "SpecialCharacter", femenuoptions::abi_special_character),
    func(0x8002_02dc, "tUserNameMenuItem::Draw", femenuoptions::abi_user_name_draw),
    func(0x8002_06d4, "tUserNameMenuItem::TransitionOff", femenuoptions::abi_user_name_transition_off),
    func(0x8002_06e0, "tUserNameMenuItem::TransitionOn", femenuoptions::abi_user_name_transition_on),
    func_ret(0x8002_0914, "tUserNameMenuItem::TransitionIsFinished", femenuoptions::abi_user_name_transition_is_finished),
    func(0x8002_0980, "tUserNameMenuItem::UpdateTransition", femenuoptions::abi_user_name_update_transition),
    func(0x8002_09fc, "tMemoryCardMenuItem::Draw", femenuoptions::abi_memory_card_draw),
    func(0x8002_0c54, "tInsideBoxControllerLeftRightSlider::ProcessInput", femenuoptions::abi_controller_slider_process_input),
    func(0x8002_0d18, "tInsideBoxControllerLeftRightSlider::~tInsideBoxControllerLeftRightSlider", femenuoptions::abi_controller_slider_dtor),
    func(0x8002_0d38, "tMemoryCardMenuItem::~tMemoryCardMenuItem", femenuoptions::abi_memory_card_dtor),
    func(0x8002_0d58, "tMenuItemGoToMenuButtonFade::~tMenuItemGoToMenuButtonFade", femenuoptions::abi_button_fade_dtor),
    func(0x8002_0d78, "tUserNameMenuItem::~tUserNameMenuItem", femenuoptions::abi_user_name_dtor),
    func(0x8002_0d98, "tMenuItemControllerLeftRightChoice::~tMenuItemControllerLeftRightChoice", femenuoptions::abi_controller_choice_dtor),
    func(0x8002_0db8, "tMenuItemOnOffLeftRightChoice::~tMenuItemOnOffLeftRightChoice", femenuoptions::abi_on_off_dtor),
    func(0x8002_0dd8, "tMenuItemDisplayLeftRightChoice::~tMenuItemDisplayLeftRightChoice", femenuoptions::abi_display_choice_dtor),
    func(0x8002_0df8, "tMenuItemSlidingActivated::~tMenuItemSlidingActivated", femenuoptions::abi_sliding_activated_dtor),
    func(0x8002_0e18, "tMenuItemLeftRightFade::~tMenuItemLeftRightFade", femenuoptions::abi_left_right_fade_dtor),
    func(0x8002_0e38, "FETextRender_SetFont", fetextrender::abi_set_font),
    func(0x8002_0eac, "FETextRender_FullTextRGB", fetextrender::abi_full_text_rgb),
    func(0x8002_1020, "FETextRender_FullText", fetextrender::abi_full_text),
    func(0x8002_10a4, "FETextRender_FullTextFade", fetextrender::abi_full_text_fade),
    func(0x8002_1174, "FETextRender_MenuTextPositioned", fetextrender::abi_menu_text_positioned),
    func(0x8002_1214, "FETextRender_MenuTextFade", fetextrender::abi_menu_text_fade),
    func(0x8002_12e0, "FETextRender_MenuTextPositionedJustify", fetextrender::abi_menu_text_positioned_justify),
    func(0x8002_1334, "FETextRender_MenuTextPositionedJustifyFade", fetextrender::abi_menu_text_positioned_justify_fade),
    func_ret(0x8002_1404, "FETextRender_WordWrapTextRGBJustify", fetextrender::abi_word_wrap_text_rgb_justify),
    func_ret(0x8002_1878, "FETextRender_WordWrapTextRGB", fetextrender::abi_word_wrap_text_rgb),
    func_ret(0x8002_18a0, "FETextRender_WordWrapTextFade", fetextrender::abi_word_wrap_text_fade),
    func_ret(0x8002_1948, "FETextRender_WordWrapText", fetextrender::abi_word_wrap_text),
    func_ret(0x8002_1984, "FETextRender_WordWrapFade", fetextrender::abi_word_wrap_fade),
    func_ret(0x8002_19ec, "FETextRender_WordWrap", fetextrender::abi_word_wrap),
    func_ret(0x8002_1a28, "FETextRender_WordWrapHeight", fetextrender::abi_word_wrap_height),
    func(0x8002_1a74, "FETextRender_Title", fetextrender::abi_title),
    func(0x8002_1b48, "FETextRender_SetABR", fetextrender::abi_set_abr),
    func(0x8002_1b70, "tTrackManager::Initialize", fetracks::abi_initialize),
    func(0x8002_1bb0, "tTrackManager::LoadTracks", fetracks::abi_load_tracks),
    func(0x8002_1bdc, "tTrackManager::SaveTracks", fetracks::abi_save_tracks),
    func(0x8002_1c04, "tTrackManager::GetTrack", fetracks::abi_get_track),
    func(0x8002_1c74, "tTrackManager::LoadDescription", fetracks::abi_load_description),
    func(0x8002_1dac, "tTrackManager::ReleaseDescription", fetracks::abi_release_description),
    func(0x8002_1dec, "tTrackManager::SetTrackAvailable", fetracks::abi_set_track_available),
    func(0x8002_1e00, "tTrackManager::SetClassAvailable", fetracks::abi_set_class_available),
    func_ret(0x8002_1e60, "tTrackManager::GetTrackByID", fetracks::abi_get_track_by_id),
    func_ret(0x8002_1eb4, "tListIteratorTrack::tListIteratorTrack", fetracks::abi_list_iterator_track),
    func(0x8002_1f0c, "tListIteratorTrack::~tListIteratorTrack", fetracks::abi_list_iterator_track_dtor),
    func_ret(0x8002_1f34, "tListIteratorTrack::TextValue", fetracks::abi_list_iterator_track_text_value),
    func(0x8002_1f74, "tListIteratorTrack::Increment", fetracks::abi_list_iterator_track_increment),
    func(0x8002_2020, "tListIteratorTrack::Decrement", fetracks::abi_list_iterator_track_decrement),
    func_ret(0x8002_20b0, "tListIteratorTrack::ValidTrack", fetracks::abi_valid_track),
    func(0x8002_3168, "InitTV", fetv::abi_init_tv),
    func(0x8002_3378, "TurnOffTV", fetv::abi_turn_off_tv),
    func(0x8002_3388, "TurnOnTV", fetv::abi_turn_on_tv),
    func(0x8002_33a0, "FECheat_HandleActivation", fecheats::abi_handle_activation),
    func(0x8002_3430, "FECheat_EncodeString", fecheats::abi_encode_string),
    func(0x8002_34bc, "FECheat_EncodeString2", fecheats::abi_encode_string2),
    func(0x8002_35f8, "FECheat_ActivateBonus", fecheats::abi_activate_bonus),
    func_ret(0x8002_3634, "FECheat_ActivateCheat", fecheats::abi_activate_cheat),
    func_ret(0x8002_373c, "FECheat_IsCheatEnabled", fecheats::abi_is_cheat_enabled),
    func(0x8002_3768, "FECheat_SaveBonus", fecheats::abi_save_bonus),
    func(0x8002_3778, "FECheat_LoadBonus", fecheats::abi_load_bonus),
    func_ret(0x8002_37f0, "FECheat_IsTheUserACryBabyCheater", fecheats::abi_is_the_user_a_cry_baby_cheater),
    func_ret(0x8002_3800, "FECheat_ActivateBonusByCode", fecheats::abi_activate_bonus_by_code),
    func(0x8002_38b8, "FEInput_VerifyControllerValues", feinput::abi_verify_controller_values),
    func_ret(0x8002_3918, "FEInput_GetNoDebounceKey", feinput::abi_get_no_debounce_key),
    func_ret(0x8002_3b94, "FEInput_GetDebounceKey", feinput::abi_get_debounce_key),
    func_ret(0x8002_3ca8, "FEInput_GetKeyFromPlayer", feinput::abi_get_key_from_player),
    func_ret(0x8002_3df0, "tListIterator::tListIterator", femenu::abi_list_iterator),
    func(0x8002_3e14, "tListIterator::~tListIterator", femenu::abi_list_iterator_dtor),
    func_ret(0x8002_3e48, "tListIterator::Value", femenu::abi_value),
    func_ret(0x8002_3e5c, "tListIterator::TextValue", femenu::abi_text_value),
    func(0x8002_3eac, "tListIterator::Increment", femenu::abi_increment),
    func(0x8002_3ef8, "tListIterator::Decrement", femenu::abi_decrement),
    func_ret(0x8002_3f80, "tListIteratorIndexed::tListIteratorIndexed", femenu::abi_indexed),
    func(0x8002_3fc4, "tListIteratorIndexed::~tListIteratorIndexed", femenu::abi_indexed_dtor),
    func_ret(0x8002_3fec, "tListIteratorIndexed::Value", femenu::abi_indexed_value),
    func_ret(0x8002_4010, "tListIteratorIndexed::TextValue", femenu::abi_indexed_text_value),
    func(0x8002_4060, "tListIteratorIndexed::Increment", femenu::abi_indexed_increment),
    func(0x8002_40c8, "tListIteratorIndexed::Decrement", femenu::abi_indexed_decrement),
    func(0x8002_4174, "tListIteratorDoubleIndexed::~tListIteratorDoubleIndexed", femenu::abi_double_indexed_dtor),
    func_ret(0x8002_419c, "tListIteratorDoubleIndexed::Value", femenu::abi_double_indexed_value),
    func_ret(0x8002_41d8, "tListIteratorDoubleIndexed::TextValue", femenu::abi_double_indexed_text_value),
    func(0x8002_4228, "tListIteratorDoubleIndexed::Increment", femenu::abi_double_indexed_increment),
    func(0x8002_42c4, "tListIteratorDoubleIndexed::Decrement", femenu::abi_double_indexed_decrement),
    func(0x8002_43b0, "tListIteratorMultiPlayer::~tListIteratorMultiPlayer", femenu::abi_multi_player_dtor),
    func_ret(0x8002_43d8, "tListIteratorMultiPlayer::Value", femenu::abi_multi_player_value),
    func_ret(0x8002_4400, "tListIteratorMultiPlayer::TextValue", femenu::abi_multi_player_text_value),
    func(0x8002_4450, "tListIteratorMultiPlayer::Increment", femenu::abi_multi_player_increment),
    func(0x8002_44b4, "tListIteratorMultiPlayer::Decrement", femenu::abi_multi_player_decrement),
    func_ret(0x8002_4554, "tListIteratorRange::tListIteratorRange", femenu::abi_range),
    func(0x8002_45b0, "tListIteratorRange::~tListIteratorRange", femenu::abi_range_dtor),
    func_ret(0x8002_45d8, "tListIteratorRange::Value", femenu::abi_range_value),
    func_ret(0x8002_45ec, "tListIteratorRange::TextValue", femenu::abi_range_text_value),
    func(0x8002_45f4, "tListIteratorRange::Increment", femenu::abi_range_increment),
    func(0x8002_461c, "tListIteratorRange::Decrement", femenu::abi_range_decrement),
    func_ret(0x8002_4644, "tListIteratorRangeIndexed::tListIteratorRangeIndexed", femenu::abi_range_indexed),
    func(0x8002_4690, "tListIteratorRangeIndexed::~tListIteratorRangeIndexed", femenu::abi_range_indexed_dtor),
    func_ret(0x8002_46b8, "tListIteratorRangeIndexed::Value", femenu::abi_range_indexed_value),
    func(0x8002_46dc, "tListIteratorRangeIndexed::Increment", femenu::abi_range_indexed_increment),
    func(0x8002_4718, "tListIteratorRangeIndexed::Decrement", femenu::abi_range_indexed_decrement),
    func_ret(0x8002_4754, "tMenuItem::tMenuItem", femenu::abi_menu_item),
    func(0x8002_4784, "tMenuItem::~tMenuItem", femenu::abi_menu_item_dtor),
    func_ret(0x8002_47b8, "tMenuItem::DebounceKeys", femenu::abi_menu_item_debounce_keys),
    func(0x8002_47c0, "tMenuItem::ProcessInput", femenu::abi_nothing),
    func(0x8002_47c8, "tMenuItem::UpdateTransition", femenu::abi_menu_item_update_transition),
    func_ret(0x8002_47e8, "tMenuItem::TransitionIsFinished", femenu::abi_finished),
    func(0x8002_47f0, "tMenuItem::UpdateSelFade", femenu::abi_update_sel_fade),
    func(0x8002_4848, "tMenuItem::Draw", femenu::abi_menu_item_draw_at),
    func(0x8002_4884, "tMenuItem::Draw(int,int,int,bool)", femenu::abi_menu_item_draw_at3),
    func_ret(0x8002_48c8, "tMenuItemInteractive::tMenuItemInteractive", femenu::abi_interactive),
    func(0x8002_48fc, "tMenuItemInteractive::~tMenuItemInteractive", femenu::abi_interactive_dtor),
    func_ret(0x8002_4924, "tMenuItemLeftRightChoice::tMenuItemLeftRightChoice", femenu::abi_left_right_choice),
    func(0x8002_4974, "tMenuItemLeftRightChoice::~tMenuItemLeftRightChoice", femenu::abi_left_right_choice_dtor),
    func(0x8002_499c, "tMenuItemLeftRightChoice::ProcessInput", femenu::abi_left_right_choice_process_input),
    func(0x8002_4a4c, "tMenuItemLeftRightChoice::Draw", femenu::abi_left_right_choice_draw),
    func_ret(0x8002_4b18, "tMenuItemLeftRightSlider::tMenuItemLeftRightSlider", femenu::abi_left_right_slider),
    func(0x8002_4b74, "tMenuItemLeftRightSlider::~tMenuItemLeftRightSlider", femenu::abi_left_right_slider_dtor),
    func_ret(0x8002_4b9c, "tMenuItemLeftRightSlider::DebounceKeys", femenu::abi_left_right_slider_debounce_keys),
    func(0x8002_4ba4, "tMenuItemLeftRightSlider::ProcessInput", femenu::abi_left_right_slider_process_input),
    func(0x8002_5204, "tMenuItemLeftRightSlider::Draw", femenu::abi_left_right_slider_draw),
    func(0x8002_5330, "tMenuItemLeftRightSlider::SetDimensions", femenu::abi_set_dimensions),
    func_ret(0x8002_5348, "tMenuItemGoToMenuButton::tMenuItemGoToMenuButton", femenu::abi_go_to_menu_button),
    func(0x8002_539c, "tMenuItemGoToMenuButton::~tMenuItemGoToMenuButton", femenu::abi_go_to_menu_button_dtor),
    func(0x8002_53c4, "tMenuItemGoToMenuButton::ProcessInput", femenu::abi_go_to_menu_button_process_input),
    func(0x8002_5454, "tMenu::tMenuConstructor", femenu::abi_menu_constructor),
    func_ret(0x8002_548c, "tMenu::tMenu", femenu::abi_menu),
    func(0x8002_54d0, "tMenu::~tMenu", femenu::abi_menu_dtor),
    func(0x8002_5504, "tMenu::Initialize", femenu::abi_initialize),
    func(0x8002_5598, "tMenu::ProcessInput", femenu::abi_process_input),
    func_ret(0x8002_58b4, "tMenu::GetNumberEnabledItems", femenu::abi_get_number_enabled_items),
    func(0x8002_591c, "tMenu::Draw", femenu::abi_draw),
    func(0x8002_59c8, "tMenu::UpdateTransition", femenu::abi_update_transition),
    func(0x8002_5a5c, "tMenu::TransitionOff", femenu::abi_nothing),
    func(0x8002_5a64, "tMenu::TransitionOn", femenu::abi_nothing),
    func_ret(0x8002_5a6c, "tMenu::TransitionIsFinished", femenu::abi_finished),
    func_ret(0x8002_5a74, "tMenu::IsSubMenu", femenu::abi_is_sub_menu),
    func_ret(0x8002_5a7c, "tMenu::DebounceKeys", femenu::abi_debounce_keys),
    func(0x8002_5ac8, "tMenuItem::TransitionOn", femenu::abi_nothing),
    func(0x8002_5ad0, "tMenuItem::TransitionOff", femenu::abi_nothing),
    func(0x8002_5ad8, "tScreen::DisplayLoadingText", fescreen::abi_display_loading_text),
    func(0x8002_5c18, "tScreen::GoNonInterlaced", fescreen::abi_go_non_interlaced),
    func(0x8002_5ce8, "tScreen::DrawBackgroundImage", fescreen::abi_draw_background_image),
    func(0x8002_5db8, "tScreen::AsyncLoadPermanentShapeFile", fescreen::abi_async_load_permanent_shape_file),
    func(0x8002_5dd8, "tScreen::AsyncLoadSwapShapeFile", fescreen::abi_async_load_swap_shape_file),
    func_ret(0x8002_5e2c, "tScreen::IsShapeFileLoaded", fescreen::abi_is_shape_file_loaded),
    func(0x8002_5fa4, "tScreen::UploadPermanentShapes", fescreen::abi_upload_permanent_shapes),
    func(0x8002_5ff0, "tScreen::UploadSwapShapes", fescreen::abi_upload_swap_shapes),
    func(0x8002_6028, "tScreen::GetShapeInfo", fescreen::abi_get_shape_info),
    func_ret(0x8002_6040, "tScreen::tScreen", fescreen::abi_screen),
    func(0x8002_6098, "tScreen::~tScreen", fescreen::abi_screen_destructor),
    func(0x8002_60cc, "tScreen::Initialize", fescreen::abi_initialize),
    func(0x8002_61c0, "tScreen::Cleanup", fescreen::abi_cleanup),
    func(0x8002_620c, "tScreen::Draw", fescreen::abi_draw),
    func(0x8002_6260, "tScreen::AsyncLoadShapeFile", fescreen::abi_async_load_shape_file),
    func(0x8002_6374, "tScreen::CancelAsyncLoad", fescreen::abi_cancel_async_load),
    func(0x8002_640c, "tScreen::InitializeShapes", fescreen::abi_initialize_shapes),
    func(0x8002_64b4, "tScreen::FreeShapes", fescreen::abi_free_shapes),
    func(0x8002_6614, "tScreen::UploadShapes", fescreen::abi_upload_shapes),
    func(0x8002_67a0, "tScreen::PreLoad", fescreen::abi_pre_load),
    func(0x8002_6824, "tScreen::TransitionOff", fescreen::abi_transition_off),
    func(0x8002_6844, "tScreen::TransitionOn", fescreen::abi_transition_on),
    func(0x8002_6868, "tScreen::UpdateTransition", fescreen::abi_update_transition),
    func_ret(0x8002_68e0, "tScreen::TransitionIsFinished", fescreen::abi_transition_is_finished),
    func(0x8002_68f8, "tScreen::ProcessInput", fescreen::abi_process_input),
    func(0x8002_6900, "tScreen::DrawForeground", fescreen::abi_draw_foreground),
    func(0x8002_6908, "tScreen::DrawBackground", fescreen::abi_draw_background),
    func(0x8002_6910, "s_upper", fetools::abi_s_upper),
    func(0x8002_6970, "s_lower", fetools::abi_s_lower),
    func(0x8002_69d0, "FeTools_DrawPSXButton", fetools::abi_draw_psx_button),
    func(0x8002_6bc8, "FeTools_init", fetools::abi_init),
    func(0x8002_6cb0, "FeTools_deinit", fetools::abi_deinit),
    func(0x8002_6d00, "FeTools_FormatMoney", fetools::abi_format_money),
    func(0x8002_6e78, "Front_ConstructAll", front::abi_construct_all),
    func(0x8002_7008, "Front_DeleteAll", front::abi_delete_all),
    func_ret(0x8002_706c, "Front_ResetSettingsForCar", front::abi_reset_settings_for_car),
    func(0x8002_70bc, "Front_ResetSettingsMenu", front::abi_reset_settings_menu),
    func(0x8002_7104, "Front_ResetPSXController", front::abi_reset_psx_controller),
    func(0x8002_7170, "Front_ResetPSXAnalogs", front::abi_reset_psx_analogs),
    func_ret(0x8002_71b4, "GetPSXPadValue", front::abi_get_psx_pad_value),
    func(0x8002_752c, "SetPads", front::abi_set_pads),
    func(0x8002_7668, "InitFrontEndStructure", front::abi_init_front_end_structure),
    func_ret(0x8002_7960, "LoadConfig", front::abi_load_config),
    func(0x8002_7968, "Front_InitialMemCardCheck", front::abi_initial_mem_card_check),
    func_ret(0x8002_79cc, "Front_SecondaryMemCardCheck", front::abi_secondary_mem_card_check),
    func(0x8002_7aa0, "Front_InitGraphics", front::abi_init_graphics),
    func(0x8002_7b18, "Front_InitGraphicsAndDisplayLoading", front::abi_init_graphics_and_display_loading),
    func_ret(0x8002_7b38, "Front_Menu", front::abi_front_menu),
    func_ret(0x8002_7dec, "OutputDisplaySettings", front::abi_output_display_settings),
    func_ret(0x8002_8004, "Front_GetLapsForType", front::abi_get_laps_for_type),
    func(0x8002_80ac, "Front_InitStream", front::abi_init_stream),
    func(0x8002_80dc, "Front_InitPlayerCars", front::abi_init_player_cars),
    func(0x8002_84a0, "Front_InitTourneyTraffic", front::abi_init_tourney_traffic),
    func(0x8002_8614, "Front_InitOpponentCars", front::abi_init_opponent_cars),
    func(0x8002_8ba8, "Front_InitMissions", front::abi_init_missions),
    func(0x8002_8cfc, "Front_InitCopCars", front::abi_init_cop_cars),
    func(0x8002_90bc, "Front_InitPerps", front::abi_init_perps),
    func(0x8002_9284, "Front_InitTrack", front::abi_init_track),
    func(0x8002_9458, "Front_InitTraffic", front::abi_init_traffic),
    func_ret(0x8002_9690, "Front_AppendPlayerCarData", front::abi_append_player_car_data),
    func_ret(0x8002_9b74, "Front_AppendOpponentData", front::abi_append_opponent_data),
    func_ret(0x8002_9ea8, "Front_AppendCopData", front::abi_append_cop_data),
    func_ret(0x8002_a0fc, "Front_AppendPerpData", front::abi_append_perp_data),
    func_ret(0x8002_a394, "Front_AppendTrafficData", front::abi_append_traffic_data),
    func_ret(0x8002_a5e4, "Front_AppendTrackData", front::abi_append_track_data),
    func_ret(0x8002_a724, "Front_EnableLocalSpeech", front::abi_enable_local_speech),
    func(0x8002_b750, "Front_GetInGameVars", front::abi_get_in_game_vars),
    func(0x8002_b84c, "SetLicensePlate", front::abi_set_license_plate),
    func(0x8002_b8f4, "SetPlayList", front::abi_set_play_list),
    func_ret(0x8002_b9e0, "Front_GetTrackRaced", front::abi_get_track_raced),
    func_ret(0x8002_ba50, "PlayerNameExist", front::abi_player_name_exist),
    func_ret(0x8002_ba7c, "PlayerName", front::abi_player_name),
    func_ret(0x8002_bb08, "PlayerNameMixedCase", front::abi_player_name_mixed_case),
    func_ret(0x8002_bb74, "tAllScreens::tAllScreens", front::abi_all_screens),
    func(0x8002_bf8c, "tAllScreens::~tAllScreens", front::abi_all_screens_dtor),
    func(0x8002_c160, "MenuExtended_SetOnePlayer", femenudefs::abi_set_one_player),
    func(0x8002_c1ac, "MenuExtended_SetTwoPlayer", femenudefs::abi_set_two_player),
    func(0x8002_c1fc, "MenuExtended_SetTestDrive", femenudefs::abi_set_test_drive),
    func(0x8002_c218, "MenuExtended_SetSingleRace", femenudefs::abi_set_single_race),
    func(0x8002_c258, "MenuExtended_SetTournament", femenudefs::abi_set_tournament),
    func(0x8002_c2a0, "MenuExtended_SetSpecialEvent", femenudefs::abi_set_special_event),
    func(0x8002_c2e8, "MenuExtended_SetSoloRace", femenudefs::abi_set_solo_race),
    func(0x8002_c308, "MenuExtended_GoToTwoPlayerSingleRace", femenudefs::abi_go_to_two_player_single_race),
    func(0x8002_c41c, "MenuExtended_SetDuelRace", femenudefs::abi_set_duel_race),
    func(0x8002_c45c, "MenuExtended_SetFullGrid", femenudefs::abi_set_full_grid),
    func_ret(0x8002_c49c, "AskTheUserToSaveTheGame", femenudefs::abi_ask_the_user_to_save_the_game),
    func(0x8002_c514, "MenuExtended_TransitionFromPostGameToMainMenu", femenudefs::abi_transition_from_post_game_to_main_menu),
    func(0x8002_c530, "MenuExtended_TransitionFromPostGameToMainMenuAndSaveGame", femenudefs::abi_transition_from_post_game_to_main_menu_and_save_game),
    func(0x8002_c57c, "MenuExtended_GoToCarSelect", femenudefs::abi_go_to_car_select),
    func(0x8002_c910, "MenuExtended_GoToDealer", femenudefs::abi_go_to_dealer),
    func(0x8002_c978, "MenuExtended_GoToSeller", femenudefs::abi_go_to_seller),
    func(0x8002_c9e0, "MenuExtended_GoToUpgrades", femenudefs::abi_go_to_upgrades),
    func_ret(0x8002_ca20, "GenericMenuSaveGame", femenudefs::abi_generic_menu_save_game),
    func_ret(0x8002_cb3c, "PinkSlipsPreSave", femenudefs::abi_pink_slips_pre_save),
    func(0x8002_cc04, "MenuExtended_GoToBestOfOne", femenudefs::abi_go_to_best_of_one),
    func(0x8002_cc60, "MenuExtended_GoToBestOfThree", femenudefs::abi_go_to_best_of_three),
    func(0x8002_ccbc, "MenuExtended_GoToBestOfFive", femenudefs::abi_go_to_best_of_five),
    func(0x8002_cd18, "MenuExtended_GoToRace", femenudefs::abi_go_to_race),
    func(0x8002_ce74, "MenuExtended_GoTo2PlayerRace", femenudefs::abi_go_to_2_player_race),
    func(0x8002_cfc0, "MenuExtended_GoToTournTrackInfo", femenudefs::abi_go_to_tourn_track_info),
    func(0x8002_d128, "MenuExtended_GoToSpecialEventTrackInfo", femenudefs::abi_go_to_special_event_track_info),
    func(0x8002_d294, "MenuExtended_EnterUserName", femenudefs::abi_enter_user_name),
    func(0x8002_d2f8, "MenuExtended_GoToShowroom", femenudefs::abi_go_to_showroom),
    func(0x8002_d338, "MenuExtended_GoToDealerShowroom", femenudefs::abi_go_to_dealer_showroom),
    func(0x8002_d378, "MenuExtended_SetHPSoloRace", femenudefs::abi_set_hp_solo_race),
    func(0x8002_d3a4, "MenuExtended_SetHPDuelRace", femenudefs::abi_set_hp_duel_race),
    func(0x8002_d3d0, "MenuExtended_SetHotPursuit", femenudefs::abi_set_hot_pursuit),
    func(0x8002_d420, "MenuExtended_SellCar", femenudefs::abi_sell_car),
    func(0x8002_d578, "MenuExtended_BuyCar", femenudefs::abi_buy_car),
    func(0x8002_d6cc, "MenuExtended_PurchaseUpgrade", femenudefs::abi_purchase_upgrade),
    func(0x8002_d80c, "MenuExtended_PurchaseUpgrade1", femenudefs::abi_purchase_upgrade1),
    func(0x8002_d82c, "MenuExtended_PurchaseUpgrade2", femenudefs::abi_purchase_upgrade2),
    func(0x8002_d84c, "MenuExtended_PurchaseUpgrade3", femenudefs::abi_purchase_upgrade3),
    func(0x8002_d86c, "MenuExtended_SaveGame", femenudefs::abi_save_game),
    func(0x8002_d8bc, "GenericMenuLoadGame", femenudefs::abi_generic_menu_load_game),
    func(0x8002_d950, "MenuExtended_LoadGame", femenudefs::abi_load_game),
    func(0x8002_da28, "MenuExtended_TierFinished", femenudefs::abi_tier_finished),
    func_ret(0x8002_daa8, "MenuExtended_DidUserWinBeTheCop", femenudefs::abi_did_user_win_be_the_cop),
    func(0x8002_dbc4, "MenuExtended_PostGameMenu", femenudefs::abi_post_game_menu),
    func(0x8002_dcc4, "MenuExtended_FinishedPlayer1GetName", femenudefs::abi_finished_player1_get_name),
    func(0x8002_ddd4, "MenuExtended_FinishedPlayer2GetName", femenudefs::abi_finished_player2_get_name),
    func(0x8002_de74, "MenuExtended_SetPinkSlips", femenudefs::abi_set_pink_slips),
    func(0x8002_df34, "MenuExtended_AwardPinkSlipsCar", femenudefs::abi_award_pink_slips_car),
    func(0x8002_e1e8, "MenuExtended_GoToGarage", femenudefs::abi_go_to_garage),
    func(0x8002_e290, "MenuExtended_GoToTournTrophyRoom", femenudefs::abi_go_to_tourn_trophy_room),
    func(0x8002_e29c, "MenuExtended_GoToSETrophyRoom", femenudefs::abi_go_to_se_trophy_room),
    func(0x8002_e2ac, "MenuExtended_SetBeginner", femenudefs::abi_set_beginner),
    func(0x8002_e2b8, "MenuExtended_SetIntermediate", femenudefs::abi_set_intermediate),
    func(0x8002_e2c8, "MenuExtended_SetExpert", femenudefs::abi_set_expert),
    func(0x8002_e2d8, "MenuExtended_ExitTourney", femenudefs::abi_exit_tourney),
    func(0x8002_e368, "MenuExtended_ExitPinkSlipsEarly", femenudefs::abi_exit_pink_slips_early),
    func(0x8002_e498, "MenuExtended_PinkSlipsContinue", femenudefs::abi_pink_slips_continue),
    func_ret(0x8002_e504, "tGlobalMenuDefs::tGlobalMenuDefs", femenudefs::abi_global_menu_defs),
    func(0x8003_1720, "~tGlobalMenuDefs", femenudefs::abi_global_menu_defs_destructor),
    func(0x8003_2378, "tTournamentManager::Initialize", fetourn::abi_initialize),
    func(0x8003_244c, "tTournamentManager::LoadDescription", fetourn::abi_load_description),
    func(0x8003_2698, "tTournamentManager::ReleaseDescription", fetourn::abi_release_description),
    func(0x8003_26d4, "tTournamentManager::UpdateTrackList", fetourn::abi_update_track_list),
    func_ret(0x8003_27a4, "tTournamentManager::GetTrackList", fetourn::abi_get_track_list),
    func(0x8003_27dc, "tTournamentManager::GetTrackToRace", fetourn::abi_get_track_to_race),
    func(0x8003_28d4, "tTournamentManager::StartNewTournament", fetourn::abi_start_new_tournament),
    func_ret(0x8003_2b0c, "tTournamentManager::IsTournamentFinished", fetourn::abi_is_tournament_finished),
    func_ret(0x8003_2ba0, "tournPointsCompare", fetourn::abi_tourn_points_compare),
    func(0x8003_2c2c, "tTournamentManager::UpdateTournFinishMoney", fetourn::abi_update_tourn_finish_money),
    func(0x8003_2d5c, "tTournamentManager::UpdateTrackFinishMoney", fetourn::abi_update_track_finish_money),
    func(0x8003_2dbc, "tTournamentManager::CalcTrackFinishDamageBill", fetourn::abi_calc_track_finish_damage_bill),
    func(0x8003_2f80, "tTournamentManager::UpdateTrackFinishPoints", fetourn::abi_update_track_finish_points),
    func_ret(0x8003_3184, "tTournamentManager::AdvanceToNextTrack", fetourn::abi_advance_to_next_track),
    func_ret(0x8003_35c8, "tTournamentManager::GetLastTrackRaced", fetourn::abi_get_last_track_raced),
    func(0x8003_3638, "tTournamentManager::SaveTournament", fetourn::abi_save_tournament),
    func(0x8003_3680, "tTournamentManager::LoadTournament", fetourn::abi_load_tournament),
    func_ret(0x8003_36c8, "tTournamentManager::GetNumCompetitors", fetourn::abi_get_num_competitors),
    func(0x8003_3714, "tTournamentManager::UpdateCarLineup", fetourn::abi_update_car_lineup),
    func_ret(0x8003_37c4, "tTournamentManager::GetTrackFinishPrize", fetourn::abi_get_track_finish_prize),
    func_ret(0x8003_399c, "tTournamentManager::GetTournamentFinishPrize", fetourn::abi_get_tournament_finish_prize),
    func(0x8003_3a10, "tTournamentManager::GetAwardInformation", fetourn::abi_get_award_information),
    func(0x8003_3a50, "tTournamentManager::UpdateAwardInformation", fetourn::abi_update_award_information),
    func_ret(0x8003_3bc8, "tTournamentManager::TournPointTotal", fetourn::abi_tourn_point_total),
    func_ret(0x8003_3be4, "tTournamentManager::PlayerRanking", fetourn::abi_player_ranking),
    func(0x8003_3c74, "tTournamentManager::CalcTierFinishPrize", fetourn::abi_calc_tier_finish_prize),
    func(0x8003_3ca0, "tTournamentManager::GetTrophyName", fetourn::abi_get_trophy_name),
    func_ret(0x8003_3d60, "tTournamentManager::ValidCar", fetourn::abi_valid_car),
    func_ret(0x8003_3ed8, "tListIteratorTournament::tListIteratorTournament", fetourn::abi_list_iterator_tournament),
    func(0x8003_3f28, "tListIteratorTournament::~tListIteratorTournament", fetourn::abi_list_iterator_tournament_dtor),
    func_ret(0x8003_3f50, "tListIteratorTournament::Value", fetourn::abi_value),
    func_ret(0x8003_3f64, "tListIteratorTournament::TextValue", fetourn::abi_text_value),
    func(0x8003_3fbc, "tListIteratorTournament::Increment", fetourn::abi_increment),
    func(0x8003_405c, "tListIteratorTournament::Decrement", fetourn::abi_decrement),
    func_ret(0x8003_40ec, "tListIteratorTournament::ValidTournament", fetourn::abi_valid_tournament),
    func(0x8003_4198, "tMissionManager::Initialize", femission::abi_initialize),
    func(0x8003_41b4, "tMissionManager::LoadDescription", femission::abi_load_description),
    func(0x8003_42e8, "tMissionManager::ReleaseDescription", femission::abi_release_description),
    func_ret(0x8003_4324, "tMissionManager::GetMissionStages", femission::abi_get_mission_stages),
    func(0x8003_438c, "tMissionManager::GetMissionToRace", femission::abi_get_mission_to_race),
    func(0x8003_43d0, "ChecksumData", fememcard::abi_checksum_data),
    func_ret(0x8003_4448, "VerifySuccessfulRead", fememcard::abi_verify_successful_read),
    func(0x8003_4520, "MakeWayForMemoryCard", fememcard::abi_make_way),
    func(0x8003_4544, "BringThatBeatBack", fememcard::abi_bring_back),
    func_ret(0x8003_4568, "Confirm", fememcard::abi_confirm),
    func_ret(0x8003_471c, "OverwriteConfirm", fememcard::abi_overwrite_confirm),
    func_ret(0x8003_4748, "OverwriteAlwaysYes", fememcard::abi_overwrite_always_yes),
    func_ret(0x8003_4750, "FormatConfirm", fememcard::abi_format_confirm),
    func(0x8003_477c, "LoadingProc", fememcard::abi_loading_proc),
    func(0x8003_4784, "LoadingRedrawProc", fememcard::abi_loading_redraw_proc),
    func(0x8003_47ac, "SavingProc", fememcard::abi_saving_proc),
    func(0x8003_47d4, "Init_Memcard", fememcard::abi_init_memcard),
    func(0x8003_4938, "DeInit_Memcard", fememcard::abi_deinit_memcard),
    func(0x8003_49e0, "Init_MemcardFile", fememcard::abi_init_memcard_file),
    func_ret(0x8003_4ac8, "SaveGame", fememcard::abi_save_game),
    func_ret(0x8003_4f70, "LoadGame", fememcard::abi_load_game),
    func_ret(0x8003_5580, "SavePinkSlipsCars", fememcard::abi_save_pink_slips_cars),
    func_ret(0x8003_5930, "SavePinkSlipsCarsWithErrorDialogs", fememcard::abi_save_pink_slips_with_dialogs),
    func(0x8003_5ca0, "tCreditManager::Setup", fecredits::abi_setup),
    func(0x8003_5cb0, "tCreditManager::Init", fecredits::abi_init),
    func(0x8003_5d90, "tCreditManager::DeInit", fecredits::abi_de_init),
    func(0x8003_5d9c, "tCreditManager::RealDeInit", fecredits::abi_real_de_init),
    func(0x8003_5ddc, "tCreditManager::Draw", fecredits::abi_draw),
    func(0x8003_5f20, "tCreditManager::Update", fecredits::abi_update),
    func(0x8003_6278, "tCreditManager::DrawPage", fecredits::abi_draw_page),
    func(0x8003_6988, "tVideoWall::Initialize", fevideowall::abi_initialize),
    func(0x8003_6a38, "tVideoWall::UpdateImages", fevideowall::abi_update_images),
    func(0x8003_6b84, "tVideoWall::SetAvailableText", fevideowall::abi_set_available_text),
    func(0x8003_6ba0, "tVideoWall::SetAvailableIcon", fevideowall::abi_set_available_icon),
    func(0x8003_6bc0, "tVideoWall::SetOffset", fevideowall::abi_set_offset),
    func(0x8003_6bcc, "tVideoWall::SetAvailable", fevideowall::abi_set_available),
    func(0x8003_6bd4, "tVideoWall::SetValid", fevideowall::abi_set_valid),
    func(0x8003_6bdc, "tVideoWall::Update", fevideowall::abi_update),
    func(0x8003_6dac, "tVideoWall::Draw", fevideowall::abi_draw),
    func(0x8003_7050, "tVideoWall::TurnOff", fevideowall::abi_turn_off),
    func(0x8003_7074, "tVideoWall::TurnOffInstant", fevideowall::abi_turn_off_instant),
    func(0x8003_70f0, "tVideoWall::TurnOn", fevideowall::abi_turn_on),
    func(0x8003_7118, "tScreenMain::SwapBackground", screenmain::abi_swap_background),
    func_ret(0x8003_72d4, "tScreenMain::DoneLoadingBackground", screenmain::abi_done_loading_background),
    func(0x8003_7350, "tScreenMain::SetState", screenmain::abi_set_state),
    func(0x8003_7624, "tScreenMain::InitDynamicImages", screenmain::abi_init_dynamic_images),
    func(0x8003_7990, "tScreenMain::ProcessInput", screenmain::abi_process_input),
    func(0x8003_7b10, "tScreenMain::DrawVideoLines", screenmain::abi_draw_video_lines),
    func(0x8003_7bb8, "tScreenMain::DrawBackground", screenmain::abi_draw_background),
    func(0x8003_8890, "tScreenMain::GetShapeInfo", screenmain::abi_get_shape_info),
    func(0x8003_88c0, "tScreenMain::PreLoad", screenmain::abi_pre_load),
    func(0x8003_895c, "tScreenMain::Initialize", screenmain::abi_initialize),
    func(0x8003_8cd0, "tScreenMain::Cleanup", screenmain::abi_cleanup),
    func(0x8003_8d3c, "~tScreenMain", screenmain::abi_destructor),
    func(0x8003_8d60, "tScreenPinkSlips::DrawBackground", screenpinkslips::abi_draw_background),
    func(0x8003_9378, "tScreenPinkSlips::GetShapeInfo", screenpinkslips::abi_get_shape_info),
    func(0x8003_9438, "tScreenPinkSlips::Initialize", screenpinkslips::abi_initialize),
    func(0x8003_9580, "tScreenPinkSlips::Cleanup", screenpinkslips::abi_cleanup),
    func(0x8003_95fc, "tScreenPinkSlips::UpdateVideoWall", screenpinkslips::abi_update_video_wall),
    func(0x8003_96a8, "tScreenPinkSlips::DrawVideoWall", screenpinkslips::abi_draw_video_wall),
    func(0x8003_9924, "tScreenPinkSlips::ProcessInput", screenpinkslips::abi_process_input),
    func(0x8003_9a60, "~tScreenPinkSlips", screenpinkslips::abi_destructor),
    func(0x8003_9a80, "DrawMoney", screenpost::abi_draw_money),
    func_ret(0x8003_9bb8, "tScreenTournamentStandings::tScreenTournamentStandings", screenpost::abi_screen_tournament_standings),
    func(0x8003_9bf0, "tScreenTournamentStandings::Initialize", screenpost::abi_initialize),
    func(0x8003_9d7c, "tScreenTournamentStandings::Cleanup", screenpost::abi_cleanup),
    func(0x8003_9d9c, "tScreenTournamentStandings::GetShapeInfo", screenpost::abi_get_shape_info),
    func(0x8003_9dc0, "tScreenTournamentStandings3item::GetShapeInfo", screenpost::abi_get_shape_info3),
    func(0x8003_9de4, "tScreenTournamentStandings::ProcessInput", screenpost::abi_process_input),
    func(0x8003_9e14, "tScreenTournamentStandings::DrawBackground", screenpost::abi_draw_background),
    func(0x8003_a6d8, "tScreenPinkSlipStandings::DrawBackground", screenpost::abi_pink_slip_draw_background),
    func(0x8003_aafc, "tScreenPinkSlipStandings::ProcessInput", screenpost::abi_pink_slip_process_input),
    func(0x8003_ab1c, "~tScreenPinkSlipStandings", screenpost::abi_pink_slip_destructor),
    func(0x8003_ab3c, "~tScreenTournamentStandings3item", screenpost::abi_destructor3),
    func(0x8003_ab5c, "~tScreenTournamentStandings", screenpost::abi_destructor),
    func(0x8003_ab80, "TransformVector", screencarselect::abi_transform_vector),
    func(0x8003_ac64, "DrawCar", screencarselect::abi_draw_car),
    func_ret(0x8003_ae58, "tScreenCarSelect::tScreenCarSelect", screencarselect::abi_screen_car_select),
    func(0x8003_af68, "~tScreenCarSelect", screencarselect::abi_screen_car_select_destructor),
    func(0x8003_af90, "tScreenCarSelect::Cleanup", screencarselect::abi_cleanup),
    func(0x8003_afd8, "tScreenCarSelect::DrawOverlay", screencarselect::abi_draw_overlay),
    func(0x8003_b874, "tScreenCarSelect::SetState", screencarselect::abi_set_state),
    func(0x8003_baf8, "tScreenCarSelect::CalcSplinePosition", screencarselect::abi_calc_spline_position),
    func(0x8003_bdb8, "tScreenCarSelect::GetShapeInfo", screencarselect::abi_get_shape_info),
    func(0x8003_be7c, "tScreenCarSelect::UpdateVideoWall", screencarselect::abi_update_video_wall),
    func(0x8003_bf4c, "tScreenCarSelect::AllocateAsyncBuffer", screencarselect::abi_allocate_async_buffer),
    func(0x8003_bf80, "tScreenCarSelect::FreeAsyncBuffer", screencarselect::abi_free_async_buffer),
    func(0x8003_bfa8, "tScreenCarSelect::InitializeVideoWall", screencarselect::abi_initialize_video_wall),
    func(0x8003_c060, "tScreenCarSelect::Initialize", screencarselect::abi_initialize),
    func(0x8003_c268, "tScreenCarSelect::ProcessInput", screencarselect::abi_process_input),
    func(0x8003_c424, "tScreenCarSelect::DrawVideoWall", screencarselect::abi_draw_video_wall),
    func_ret(0x8003_c530, "tScreenCarSelect::GetCar", screencarselect::abi_get_car),
    func(0x8003_c7b0, "tScreenCarSelect::SetBrightness", screencarselect::abi_set_brightness),
    func(0x8003_c7c4, "tScreenCarSelect::UpdateBrightness", screencarselect::abi_update_brightness),
    func(0x8003_c848, "tScreenCarSelect::DrawBackground", screencarselect::abi_draw_background),
    func(0x8003_c978, "tScreenCarSelect::DrawSliders", screencarselect::abi_draw_sliders),
    func(0x8003_cae4, "tScreenCarSelect::DrawForeground", screencarselect::abi_draw_foreground),
    func(0x8003_d3bc, "tScreenCarSelectDuel::PreLoad", screencarselect::abi_duel_pre_load),
    func(0x8003_d4e4, "tScreenCarSelectDuel::AllocateAsyncBuffer", screencarselect::abi_duel_allocate_async_buffer),
    func(0x8003_d4ec, "tScreenCarSelectDuel::FreeAsyncBuffer", screencarselect::abi_duel_free_async_buffer),
    func(0x8003_d518, "tScreenCarSelectDuel::InitializeVideoWall", screencarselect::abi_duel_initialize_video_wall),
    func(0x8003_d634, "tScreenCarSelectDuel::Initialize", screencarselect::abi_duel_initialize),
    func(0x8003_d664, "tScreenCarSelectDuel::Cleanup", screencarselect::abi_duel_cleanup),
    func(0x8003_d694, "tScreenCarSelectDuel::DrawVideoWall", screencarselect::abi_duel_draw_video_wall),
    func(0x8003_d7ec, "tScreenCarSelectDuel::DrawOpponentVideoWall", screencarselect::abi_duel_draw_opponent_video_wall),
    func(0x8003_d910, "tScreenCarSelectDuel::GetShapeInfo", screencarselect::abi_duel_get_shape_info),
    func(0x8003_d9cc, "tScreenCarSelectDuel::UpdateVideoWall", screencarselect::abi_duel_update_video_wall),
    func(0x8003_da9c, "tScreenCarSelectDuel::UpdateOpponentVideoWall", screencarselect::abi_duel_update_opponent_video_wall),
    func(0x8003_db20, "tScreenCarSelectDuel::DrawBackground", screencarselect::abi_duel_draw_background),
    func(0x8003_e198, "tScreenCarSelectDuel::DrawForeground", screencarselect::abi_duel_draw_foreground),
    func_ret(0x8003_e370, "tScreenCarSelectTwoPlayer::GetCar", screencarselect::abi_two_player_get_car),
    func(0x8003_e4c0, "tScreenCarSelectTwoPlayer::DrawVideoWall", screencarselect::abi_two_player_draw_video_wall),
    func(0x8003_e660, "tScreenCarSelectTwoPlayer::GetShapeInfo", screencarselect::abi_two_player_get_shape_info),
    func(0x8003_e748, "tScreenCarSelectTwoPlayer::UpdateVideoWall", screencarselect::abi_two_player_update_video_wall),
    func(0x8003_e81c, "tScreenCarSelectTwoPlayer::TurnOffVideoWall", screencarselect::abi_two_player_turn_off_video_wall),
    func(0x8003_e83c, "tScreenCarSelectTwoPlayer::DrawBackground", screencarselect::abi_two_player_draw_background),
    func(0x8003_ed94, "tScreenCarSelectTwoPlayer::DrawForeground", screencarselect::abi_two_player_draw_foreground),
    func(0x8003_efd0, "tScreenCarSelectTwoPlayer::SetDialog", screencarselect::abi_two_player_set_dialog),
    func(0x8003_f090, "tScreenCarSelectTwoPlayer::AllocateAsyncBuffer", screencarselect::abi_two_player_allocate_async_buffer),
    func(0x8003_f0c4, "tScreenCarSelectTwoPlayer::InitializeVideoWall", screencarselect::abi_two_player_initialize_video_wall),
    func(0x8003_f140, "tScreenCarSelectTwoPlayer::Initialize", screencarselect::abi_two_player_initialize),
    func(0x8003_f1c0, "tScreenCarSelectTwoPlayer::Cleanup", screencarselect::abi_two_player_cleanup),
    func_ret(0x8003_f1f0, "tScreenPinkSlipsCarSelect::GetCar", screencarselect::abi_pink_get_car),
    func(0x8003_f280, "tScreenPinkSlipsCarSelect::DrawBackground", screencarselect::abi_pink_draw_background),
    func(0x8003_f2c4, "tScreenPinkSlipsCarSelect::DoMemCardStuff", screencarselect::abi_pink_do_mem_card_stuff),
    func(0x8003_f5c8, "tScreenPinkSlipsCarSelect::DrawForeground", screencarselect::abi_pink_draw_foreground),
    func(0x8003_f5e8, "tScreenPinkSlipsCarSelect::Initialize", screencarselect::abi_pink_initialize),
    func(0x8003_f658, "tScreenPinkSlipsCarSelect::Cleanup", screencarselect::abi_pink_cleanup),
    func(0x8003_f6ac, "tScreenPinkSlipsCarSelect::SetDialog", screencarselect::abi_pink_set_dialog),
    func(0x8003_f93c, "tScreenPinkSlipsCarSelect::ProcessInput", screencarselect::abi_pink_process_input),
    func(0x8003_fa24, "tScreenPinkSlipsCarSelect::GetShapeInfo", screencarselect::abi_pink_get_shape_info),
    func(0x8003_fa9c, "~tScreenPinkSlipsCarSelect", screencarselect::abi_pink_destructor),
    func(0x8003_fae0, "~tScreenCarSelectTwoPlayer", screencarselect::abi_two_player_destructor),
    func(0x8003_fb24, "~tScreenCarSelectDuel", screencarselect::abi_duel_destructor),
    func_ret(0x8003_fb48, "tScreenTournSelect::tScreenTournSelect", screentournselect::abi_screen_tourn_select),
    func(0x8003_fb7c, "~tScreenTournSelect", screentournselect::abi_destructor),
    func(0x8003_fba4, "tScreenTournSelect::GetShapeInfo", screentournselect::abi_get_shape_info),
    func(0x8003_fc70, "tScreenTournSelect::Initialize", screentournselect::abi_initialize),
    func(0x8003_feb8, "tScreenTournSelect::Cleanup", screentournselect::abi_cleanup),
    func(0x8003_fef8, "tScreenTournSelect::UpdateVideoWall", screentournselect::abi_update_video_wall),
    func(0x8003_ff9c, "tScreenTournSelect::DrawVideoWall", screentournselect::abi_draw_video_wall),
    func(0x8004_022c, "tScreenTournSelect::DrawBackground", screentournselect::abi_draw_background),
    func(0x8004_08a8, "tScreenTournSelect::DrawForeground", screentournselect::abi_draw_foreground),
    func_ret(0x8004_0950, "tScreenTrophyRoom::tScreenTrophyRoom", screentrophyroom::abi_screen_trophy_room),
    func(0x8004_0990, "~tScreenTrophyRoom", screentrophyroom::abi_destructor),
    func(0x8004_09b8, "tScreenTrophyRoom::GetShapeInfo", screentrophyroom::abi_get_shape_info),
    func(0x8004_0a78, "tScreenTrophyRoom::ProcessInput", screentrophyroom::abi_process_input),
    func(0x8004_0c88, "tScreenTrophyRoom::PreLoad", screentrophyroom::abi_pre_load),
    func(0x8004_0cf0, "tScreenTrophyRoom::Initialize", screentrophyroom::abi_initialize),
    func(0x8004_0ec8, "tScreenTrophyRoom::Cleanup", screentrophyroom::abi_cleanup),
    func(0x8004_0ef8, "tScreenTrophyRoom::DrawBackground", screentrophyroom::abi_draw_background),
    func(0x8004_130c, "tScreenTrophyRoom::LoadTrophy", screentrophyroom::abi_load_trophy),
    func(0x8004_13e8, "tScreenTrophyInfo::GetShapeInfo", screentrophyinfo::abi_get_shape_info),
    func(0x8004_1518, "tScreenTrophyInfo::DrawBackground", screentrophyinfo::abi_draw_background),
    func(0x8004_19c0, "~tScreenTrophyInfo", screentrophyinfo::abi_destructor),
    func(0x8004_19e0, "tScreenTrackSelect::DrawBackground", screentracks::abi_draw_background),
    func(0x8004_1e8c, "tScreenTrackSelect::GetShapeInfo", screentracks::abi_get_shape_info),
    func(0x8004_1f4c, "tScreenTrackSelect::Initialize", screentracks::abi_initialize),
    func(0x8004_2108, "tScreenTrackSelect::Cleanup", screentracks::abi_cleanup),
    func(0x8004_2148, "tScreenTrackSelect::SetBrightness", screentracks::abi_set_brightness),
    func(0x8004_2178, "tScreenTrackSelect::UpdateBrightness", screentracks::abi_update_brightness),
    func(0x8004_2268, "tScreenTrackSelect::UpdateVideoWall", screentracks::abi_update_video_wall),
    func(0x8004_22dc, "tScreenTrackSelect::DrawVideoWall", screentracks::abi_draw_video_wall),
    func(0x8004_23ec, "tScreenTrackSelect::ProcessInput", screentracks::abi_process_input),
    func(0x8004_25b4, "~tScreenTrackSelect", screentracks::abi_destructor),
    func(0x8004_25d8, "tScreenTrackInfo::GetShapeInfo", screentrackinfo::abi_get_shape_info),
    func(0x8004_2678, "tScreenTrackInfo::DrawBackground", screentrackinfo::abi_draw_background),
    func(0x8004_2900, "tScreenTrackInfo::Initialize", screentrackinfo::abi_initialize),
    func(0x8004_296c, "tScreenTrackInfo::ProcessInput", screentrackinfo::abi_process_input),
    func(0x8004_2a08, "~tScreenTrackInfo", screentrackinfo::abi_destructor),
    func(0x8004_2a28, "tScreenTrackRecords::GetShapeInfo", screentrackrecords::abi_get_shape_info),
    func(0x8004_2a4c, "tScreenTrackRecords::Initialize", screentrackrecords::abi_initialize),
    func(0x8004_2aa4, "tScreenTrackRecords::Cleanup", screentrackrecords::abi_cleanup),
    func(0x8004_2ae0, "tScreenTrackRecords::DrawOneRecord", screentrackrecords::abi_draw_one_record),
    func(0x8004_2d50, "tScreenTrackRecords::DrawRecords", screentrackrecords::abi_draw_records),
    func(0x8004_2f30, "tScreenTrackRecords::DrawBackground", screentrackrecords::abi_draw_background),
    func(0x8004_34e0, "~tScreenTrackRecords", screentrackrecords::abi_destructor),
    func(0x8004_3500, "tScreenControllerConfig::ClearActuators", screencontroller::abi_clear_actuators),
    func(0x8004_350c, "tScreenControllerConfig::SetActuators", screencontroller::abi_set_actuators),
    func(0x8004_3594, "tScreenControllerConfig::TurnOffShakers", screencontroller::abi_turn_off_shakers),
    func(0x8004_359c, "tScreenControllerConfig::ShakeIt", screencontroller::abi_shake_it),
    func(0x8004_3638, "Controller_SetRamp", screencontroller::abi_controller_set_ramp),
    func_ret(0x8004_3784, "tScreenControllerConfig::AnimKeyPoints", screencontroller::abi_anim_key_points),
    func(0x8004_37e8, "tScreenControllerConfig::CheckConfigs", screencontroller::abi_check_configs),
    func(0x8004_3ad4, "tScreenControllerConfig::SwapInController", screencontroller::abi_swap_in_controller),
    func(0x8004_3b60, "tScreenControllerConfig::SetCurrentController", screencontroller::abi_set_current_controller),
    func_ret(0x8004_3ed8, "tScreenControllerConfig::CalcAnimFrame", screencontroller::abi_calc_anim_frame),
    func(0x8004_3f64, "tScreenControllerConfig::ActualDrawController", screencontroller::abi_actual_draw_controller),
    func(0x8004_41cc, "tScreenControllerConfig::DrawController", screencontroller::abi_draw_controller),
    func(0x8004_4edc, "tScreenControllerConfig::HorzVertLine", screencontroller::abi_horz_vert_line),
    func(0x8004_5128, "tScreenControllerConfig::DrawArrow", screencontroller::abi_draw_arrow),
    func(0x8004_5228, "tScreenControllerConfig::DrawBackground", screencontroller::abi_draw_background),
    func(0x8004_544c, "tScreenControllerConfig::DrawForeground", screencontroller::abi_draw_foreground),
    func(0x8004_5ab4, "tScreenControllerConfig::GetShapeInfo", screencontroller::abi_get_shape_info),
    func(0x8004_5ae4, "tScreenControllerConfig::Initialize", screencontroller::abi_initialize),
    func(0x8004_5c08, "tScreenControllerConfig::Cleanup", screencontroller::abi_cleanup),
    func_ret(0x8004_5c44, "tScreenControllerConfig::tScreenControllerConfig", screencontroller::abi_screen_controller),
    func_ret(0x8004_5c94, "tScreenControllerConfig::GetHelpText", screencontroller::abi_get_help_text),
    func(0x8004_5cf4, "~tScreenControllerConfig", screencontroller::abi_screen_controller_destructor),
    func(0x8004_5d38, "tScreenDisplay::DrawBackground", screendisplay::abi_draw_background),
    func(0x8004_5dc4, "tScreenDisplay::GetShapeInfo", screendisplay::abi_get_shape_info),
    func(0x8004_5dec, "~tScreenDisplay", screendisplay::abi_destructor),
    func(0x8004_5e10, "tScreenAudio::PlaySound", screenaudio::abi_play_sound),
    func(0x8004_61b0, "tScreenAudio::DrawForeground", screenaudio::abi_draw_foreground),
    func(0x8004_62c0, "tScreenAudio::DrawBackground", screenaudio::abi_draw_background),
    func(0x8004_6528, "tScreenAudio::GetShapeInfo", screenaudio::abi_get_shape_info),
    func_ret(0x8004_654c, "tScreenAudio::tScreenAudio", screenaudio::abi_screen_audio),
    func(0x8004_658c, "tScreenAudio::Initialize", screenaudio::abi_initialize),
    func(0x8004_65ec, "tScreenAudio::Cleanup", screenaudio::abi_cleanup),
    func(0x8004_66e4, "~tScreenAudio", screenaudio::abi_destructor),
    func(0x8004_6708, "tScreenMemcard::GetShapeInfo", screenmemcard::abi_get_shape_info),
    func(0x8004_672c, "tScreenMemcard::DrawIcon", screenmemcard::abi_draw_icon),
    func(0x8004_6924, "tScreenMemcard::LoadIcon", screenmemcard::abi_load_icon),
    func(0x8004_6c80, "tScreenMemcard::DrawVerticalLine", screenmemcard::abi_draw_vertical_line),
    func(0x8004_6d34, "tScreenMemcard::DrawHorizontalLine", screenmemcard::abi_draw_horizontal_line),
    func(0x8004_6df4, "tScreenMemcard::PlaceIcons", screenmemcard::abi_place_icons),
    func(0x8004_7148, "tScreenMemcard::DrawMemCardStuff", screenmemcard::abi_draw_mem_card_stuff),
    func(0x8004_76a4, "tScreenMemcard::SetEnablings", screenmemcard::abi_set_enablings),
    func(0x8004_7858, "tScreenMemcard::DrawBackground", screenmemcard::abi_draw_background),
    func(0x8004_7ec0, "tScreenMemcard::DrawForeground", screenmemcard::abi_draw_foreground),
    func_ret(0x8004_7fcc, "tScreenMemcard::tScreenMemcard", screenmemcard::abi_screen_memcard),
    func(0x8004_8010, "tScreenMemcard::ReleaseIcons", screenmemcard::abi_release_icons),
    func(0x8004_8088, "tScreenMemcard::Initialize", screenmemcard::abi_initialize),
    func(0x8004_8230, "tScreenMemcard::Cleanup", screenmemcard::abi_cleanup),
    func(0x8004_82c0, "~tScreenMemcard", screenmemcard::abi_screen_memcard_destructor),
    func(0x8004_82e0, "tScreenCongrats::Cleanup", screencongrats::abi_cleanup),
    func(0x8004_830c, "tScreenCongrats::GetShapeInfo", screencongrats::abi_get_shape_info),
    func_ret(0x8004_855c, "tScreenCongrats::GetCar", screencongrats::abi_get_car),
    func(0x8004_85b4, "tScreenCongrats::DrawBackground", screencongrats::abi_draw_background),
    func(0x8004_8e28, "tScreenCongrats::DrawForeground", screencongrats::abi_draw_foreground),
    func(0x8004_8e30, "tScreenCongrats::CalculatePrizes", screencongrats::abi_calculate_prizes),
    func(0x8004_8ea4, "tScreenCongrats::Initialize", screencongrats::abi_initialize),
    func(0x8004_8f68, "tScreenCongrats::ProcessInput", screencongrats::abi_process_input),
    func(0x8004_8fc0, "tScreenPinkSlipCongrats::DrawCongratsMessage", screencongrats::abi_pink_slip_draw_congrats_message),
    func_ret(0x8004_90dc, "tScreenPinkSlipCongrats::GetCar", screencongrats::abi_pink_slip_get_car),
    func(0x8004_9154, "tScreenPinkSlipCongrats::CalculatePrizes", screencongrats::abi_pink_slip_calculate_prizes),
    func(0x8004_9264, "tScreenPinkSlipCongrats::Initialize", screencongrats::abi_pink_slip_initialize),
    func(0x8004_92bc, "tScreenPinkSlipCongrats::Cleanup", screencongrats::abi_pink_slip_cleanup),
    func(0x8004_92e8, "tScreenTournamentTrophy::ProcessInput", screencongrats::abi_trophy_process_input),
    func_ret(0x8004_9330, "tScreenTournamentTrophy::GetCar", screencongrats::abi_trophy_get_car),
    func(0x8004_93dc, "tScreenTournamentTrophy::DrawCongratsMessage", screencongrats::abi_trophy_draw_congrats_message),
    func(0x8004_96c0, "tScreenTournamentTrophy::CalculatePrizes", screencongrats::abi_trophy_calculate_prizes),
    func_ret(0x8004_9900, "tScreenBeTheCopCongrats::GetCar", screencongrats::abi_cop_get_car),
    func(0x8004_9960, "tScreenBeTheCopCongrats::CalculatePrizes", screencongrats::abi_cop_calculate_prizes),
    func(0x8004_99b0, "tScreenBeTheCopCongrats::DrawCongratsMessage", screencongrats::abi_cop_draw_congrats_message),
    func_ret(0x8004_9a70, "tScreenTournamentCongrats::GetCar", screencongrats::abi_tier_get_car),
    func(0x8004_9ac4, "tScreenTournamentCongrats::CalculatePrizes", screencongrats::abi_tier_calculate_prizes),
    func(0x8004_9b5c, "tScreenTournamentCongrats::DrawCongratsMessage", screencongrats::abi_tier_draw_congrats_message),
    func(0x8004_9c28, "~tScreenTournamentCongrats", screencongrats::abi_tier_destructor),
    func(0x8004_9c48, "~tScreenBeTheCopCongrats", screencongrats::abi_cop_destructor),
    func(0x8004_9c68, "~tScreenPinkSlipCongrats", screencongrats::abi_pink_slip_destructor),
    func(0x8004_9c88, "~tScreenTournamentTrophy", screencongrats::abi_trophy_destructor),
    func(0x8004_9ca8, "~tScreenCongrats", screencongrats::abi_destructor),
    func_ret(0x8004_9cc8, "StatChk_IsRecordLapTime", statchk::abi_is_record_lap_time),
    func(0x8004_a004, "StatChk_SaveRecordLapTime", statchk::abi_save_record_lap_time),
    func_ret(0x8004_a194, "StatChk_IsTopTime", statchk::abi_is_top_time),
    func(0x8004_a640, "StatChk_SaveTopTime", statchk::abi_save_top_time),
    func(0x8004_ad14, "StatChk_ClearNewRecords", statchk::abi_clear_new_records),
    func(0x8004_ad40, "Stattool_nCreateIndex", stattool::abi_n_create_index),
    func(0x8004_ae74, "Stattool_ParseTime", stattool::abi_parse_time),
    func(0x8004_af8c, "UserNameUpperCaseOneLetter", stattool::abi_user_name_upper_case_one_letter),
    func(0x8004_b0ac, "Stattool_SamNelsonsUpperLowerStringConverterForRecords", stattool::abi_title_case),
    func(0x8004_b128, "StatTool_UpperCaseItKeepingInMindThoseBloodySpecialCharacters", stattool::abi_upper_case),
    func(0x8004_b174, "Stattool_GetAllDefaultRecords", stattool::abi_get_all_default_records),
    func(0x8004_b26c, "Stattool_ReadDefaultRecords", stattool::abi_read_default_records),
    func(0x8004_b2f4, "Stattool_GetRecords", stattool::abi_get_records),
    func_ret(0x8004_b33c, "Stattool_ReturnRecordLapTime", stattool::abi_return_record_lap_time),
    func_ret(0x8004_b3a0, "Stattool_CheckForHumanCar", stattool::abi_check_for_human_car),
    func_ret(0x8004_b428, "Stattool_GetAINameFromPersonality", stattool::abi_get_ai_name_from_personality),
    func(0x8004_b468, "tScreenUserName::Initialize", screenusername::abi_initialize),
    func(0x8004_b528, "tScreenUserName::GetShapeInfo", screenusername::abi_get_shape_info),
    func(0x8004_b588, "tScreenUserName::DrawVerticalLine", screenusername::abi_draw_vertical_line),
    func(0x8004_b608, "tScreenUserName::DrawHorizontalLine", screenusername::abi_draw_horizontal_line),
    func(0x8004_b68c, "tScreenUserName::DrawBackground", screenusername::abi_draw_background),
    func(0x8004_bcb4, "~tScreenUserName", screenusername::abi_destructor),
    func_ret(0x8004_bcd8, "CalcFadeVal(int,int,int)", fefades::abi_fade),
    func_ret(0x8004_bd68, "CalcFadeVal(int,int)", fefades::abi_fade_out),
    func_ret(0x8004_bd8c, "CalcFadeVal(int,int,int,int)", fefades::abi_fade_faded),
    func_ret(0x8004_bdc0, "CalcTextFadeUnselToSel", fefades::abi_text_fade_unsel_to_sel),
    func_ret(0x8004_be30, "CalcTextFadeSelToHi", fefades::abi_text_fade_sel_to_hi),
    func(0x8004_beb4, "CalcOnOffFade", fefades::abi_on_off_fade),
    func(0x8004_c018, "PSXDrawBrightEndLine", felines::abi_draw_bright_end_line),
    func(0x8004_c058, "PSXTransDrawBrightEndLine", felines::abi_draw),
    func(0x8004_c490, "DrawShape_NFS4RoundRectangle", drawshp::abi_round_rectangle),
    func(0x8004_c69c, "DrawShape_NFS4Rectangle", drawshp::abi_rectangle),
    func(0x8004_c9bc, "DrawShape_NFS4TransRectangle", drawshp::abi_trans_rectangle),
    func_ret(0x8004_d3dc, "play_movie", movie::abi_play_movie),
    func(0x8004_dbc0, "FeDraw_SetABRMode", mmeffect::abi_set_abr_mode),
    func(0x8004_dc60, "Quick_DD", psxfront::abi_quick_dd),
    func(0x8004_dca4, "PSXFront_AllocateDrawMemory", psxfront::abi_allocate_draw_memory),
    func(0x8004_dd18, "PSXFront_FreeDrawMemory", psxfront::abi_free_draw_memory),
    func(0x8004_dd38, "InitializeSpinningCars", psxfront::abi_initialize_spinning_cars),
    func(0x8004_deb4, "CleanupSpinningCars", psxfront::abi_cleanup_spinning_cars),
    func(0x8004_dfac, "CleanupSpinningCarsMenu", psxfront::abi_cleanup_spinning_cars_menu),
    func(0x8004_e11c, "DoLanguageScreen", psxfront::abi_do_language_screen),
    func(0x8004_e158, "PSXExitFrontend", psxfront::abi_psx_exit_frontend),
    func(0x8004_e1b0, "PSX_AllocShapes", psxfront::abi_psx_alloc_shapes),
    func(0x8004_e1f4, "Init_RenderingEnvironment", psxfront::abi_init_rendering_environment),
    func(0x8004_e2e0, "Init_PSX_FrontEnd", psxfront::abi_init_psx_front_end),
    func(0x8004_e420, "AdjustShapeDrawing", psxfront::abi_adjust_shape_drawing),
    func(0x8004_eb20, "DrawFlatShape", psxfront::abi_draw_flat_shape),
    func(0x8004_eb60, "DrawShapeExtended", psxfront::abi_draw_shape_extended),
    func(0x8004_ec64, "ScaleGouraudShape", psxfront::abi_scale_gouraud_shape),
    func(0x8004_ef20, "ScaleFlatShape", psxfront::abi_scale_flat_shape),
    func(0x8004_ef70, "ScaleShapeExtended", psxfront::abi_scale_shape_extended),
    func(0x8004_f09c, "LoadAllHelpShapes", psxfront::abi_load_all_help_shapes),
    func(0x8004_f49c, "FontUpsideDownBlit", psxfront::abi_font_upside_down_blit),
    func(0x8004_f5e8, "FETexture_LoadPmxAtOffset", fetexture::abi_load_pmx_at_offset),
    func(0x8004_f8a0, "FETexture_LoadPmx", fetexture::abi_load_pmx),
    func(0x8004_f8c8, "MCRD_init", memcard::abi_init),
    func_ret(0x8004_f968, "iMCRD_InitCard", memcard::abi_init_card),
    func(0x8004_f9c0, "MCRD_restore", memcard::abi_restore),
    func(0x8004_fa04, "MCRD_getopts", memcard::abi_getopts),
    func(0x8004_fa7c, "MCRD_setopts", memcard::abi_setopts),
    func(0x8004_fba0, "MCRD_loadfile", memcard::abi_loadfile),
    func_ret(0x8004_fc8c, "iMCRD_DoFileLoad", memcard::abi_do_file_load),
    func_ret(0x8004_ff34, "MCRD_savefile", memcard::abi_savefile),
    func_ret(0x8005_0170, "iMCRD_DoFileWrite", memcard::abi_do_file_write),
    func_ret(0x8005_02d4, "iMCRD_DoFileDelete", memcard::abi_do_file_delete),
    func_ret(0x8005_0368, "MCRD_handlecardevents", memcard::abi_handlecardevents),
    func_ret(0x8005_06b4, "MCRD_fileexists", memcard::abi_fileexists),
    func_ret(0x8005_072c, "MCRD_getcard", memcard::abi_getcard),
    func(0x8005_0774, "iMCRD_timersub", memcard::abi_timersub),
    func_ret(0x8005_07a4, "garyMemCardGrabBlocks", memcard::abi_grab_blocks),
    func_ret(0x8005_082c, "iMCRD_LoadCard", memcard::abi_load_card),
    func_ret(0x8005_0928, "iMCRD_FormatCard", memcard::abi_format_card),
    func_ret(0x8005_09b4, "iMCRD_HandleError", memcard::abi_handle_error),
    func_ret(0x8005_0bd0, "iMCRD_DefaultCBProc1", memcard::abi_default_cb),
    func_ret(0x8005_0bd8, "ascii2sjis", memcard::abi_ascii2sjis),
    func_ret(0x8005_0cf8, "sjis2ascii", memcard::abi_sjis2ascii),
    func(0x8005_816c, "AI_StartUp", ai::abi_nothing),
    func(0x8005_8174, "AI_CleanUp", ai::abi_nothing),
    func(0x8005_817c, "AI_Main_OverallSetup", ai::abi_nothing),
    func(0x8005_8184, "AI_CarAvoidance", ai::abi_car_avoidance),
    func(0x8005_81c8, "AI_WorldAvoidance", ai::abi_world_avoidance),
    func(0x8005_8214, "AI_InitAIInfo", ai::abi_init_ai_info),
    func(0x8005_825c, "AI_GenericBeginCycle", ai::abi_generic_begin_cycle),
    func(0x8005_8284, "AI_GenericCycle", ai::abi_generic_cycle),
    func(0x8005_82d8, "AI_GenericEndCycle", ai::abi_generic_end_cycle),
    func(0x8005_830c, "AI_TargetLane", ai::abi_target_lane),
    func(0x8005_835c, "AI_ClearLaneMerits", ai::abi_clear_lane_merits),
    func(0x8005_8384, "AI_DoReactions", ai::abi_do_reactions),
    func(0x8005_85c4, "AI_CheckForPlayerActions", ai::abi_check_for_player_actions),
    func(0x8005_8804, "AI_DoReactionsAndBehavior", ai::abi_do_reactions_and_behavior),
    func(0x8005_88c8, "AI_OpponentBlockPlayer", ai::abi_opponent_block_player),
    func(0x8005_8ac4, "AI_CheckForBarriers", ai::abi_check_for_barriers),
    func(0x8005_8e84, "AI_SetupOncomingLaneDemerits", ai::abi_setup_oncoming_lane_demerits),
    func(0x8005_8fdc, "AI_HandleChangeInNumLanes", ai::abi_handle_change_in_num_lanes),
    func(0x8005_914c, "AI_HandleShouldersAndOffRoad", ai::abi_handle_shoulders_and_off_road),
    func(0x8005_9510, "AI_CalculateLaneSpeeds", ai::abi_calculate_lane_speeds),
    func(0x8005_98a4, "AI_CalcMeritsBasedOnSpeed", ai::abi_calc_merits_based_on_speed),
    func(0x8005_9c24, "AI_CheckForClearLanes", ai::abi_check_for_clear_lanes),
    func(0x8005_9e94, "AI_CalcBestLineMerits", ai::abi_calc_best_line_merits),
    func(0x8005_9f1c, "AI_AddCollidableObjects", ai::abi_add_collidable_objects),
    func(0x8005_a118, "AI_AvoidObjects", ai::abi_avoid_objects),
    func(0x8005_a14c, "AI_AvoidSpikeBelt", ai::abi_avoid_spike_belt),
    func(0x8005_a1d4, "AI_SubmitObstacle", ai::abi_submit_obstacle),
    func(0x8005_a378, "AI_HandleTrafficHonking", ai::abi_handle_traffic_honking),
    func(0x8005_a47c, "AI_CheckForCarsOnSide", ai::abi_check_for_cars_on_side),
    func(0x8005_a5cc, "AI_ProcessObservationsAndChooseLane", ai::abi_process_observations_and_choose_lane),
    func(0x8005_a610, "AI_ChooseBestLane", ai::abi_choose_best_lane),
    func_ret(0x8005_a69c, "AI_CheckPreferredLateralPosition", ai::abi_check_preferred_lateral_position),
    func_ret(0x8005_a850, "AI_TryToShareLanes", ai::abi_try_to_share_lanes),
    func(0x8005_a94c, "AI_CalculateDesiredLatPosition", ai::abi_calculate_desired_lat_position),
    func(0x8005_ab80, "AI_CalculateAdjustedDesiredSpeed", ai::abi_calculate_adjusted_desired_speed),
    func_ret(0x8005_ae18, "AI_IsMellowZone", ai::abi_is_mellow_zone),
    func(0x8005_af04, "AI_KeepCarsInLane", ai::abi_keep_cars_in_lane),
    func(0x8005_af14, "AI_PushFinishedCarsToSide", ai::abi_push_finished_cars_to_side),
    func(0x8005_b0b0, "AI_MaybeChangeLaneSlack", ai::abi_maybe_change_lane_slack),
    func(0x8005_b1cc, "AI_ChooseNewLaneSlack", ai::abi_choose_new_lane_slack),
    func_ret(0x8005_b218, "AIDelayCar::AIDelayCar", aidelaycar::abi_construct),
    func(0x8005_b24c, "AIDelayCar::SetNewTargetCar", aidelaycar::abi_set_new_target_car),
    func(0x8005_b334, "AIDelayCar::Update", aidelaycar::abi_update),
    func(0x8005_b584, "AIHigh_StartUp", aihigh::abi_start_up),
    func(0x8005_b92c, "AIHigh_Restart1", aihigh::abi_restart1),
    func(0x8005_b94c, "AIHigh_Restart2", aihigh::abi_restart2),
    func(0x8005_b974, "AIHigh_CleanUp", aihigh::abi_clean_up),
    func(0x8005_ba00, "AIHigh_Execute", aihigh::abi_execute),
    func_ret(0x8005_bb08, "AIHigh_Base::AIHigh_Base", aihigh::abi_base_construct),
    func(0x8005_bbb4, "AIHigh_Base::~AIHigh_Base", aihigh::abi_base_destruct),
    func(0x8005_bc28, "AIHigh_BTC_Perp::~AIHigh_BTC_Perp", aihigh::abi_btc_perp_destruct),
    func(0x8005_bc50, "AIHigh_None::HighExecute", aihigh::abi_nothing),
    func(0x8005_bc58, "AIHigh_None::~AIHigh_None", aihigh::abi_none_destruct),
    func(0x8005_bc78, "AIState_None::Execute", aihigh::abi_nothing),
    func(0x8005_bc80, "AIState_None::~AIState_None", aihigh::abi_state_none_destruct),
    func_ret(0x8005_bcb4, "AIState_Base::TestForRelease", aihigh::abi_state_test_for_release),
    func(0x8005_bcbc, "AIState_Base::~AIState_Base", aihigh::abi_state_none_destruct),
    func(0x8005_bcf0, "AIHigh_BasicPerp::CheckForCrimes", aih_basicperp::abi_check_for_crimes),
    func_ret(0x8005_bf7c, "AIHigh_BasicPerp::CheckIfCaught", aih_basicperp::abi_check_if_caught),
    func(0x8005_c56c, "AIHigh_BasicPerp::RemoveCloseCops", aih_basicperp::abi_remove_close_cops),
    func(0x8005_c6bc, "AIHigh_BasicPerp::RemoveChaser", aih_basicperp::abi_remove_chaser),
    func(0x8005_c6f8, "AIHigh_BasicPerp::AddChaser", aih_basicperp::abi_add_chaser),
    func_ret(0x8005_c74c, "AIHigh_BasicPerp::CheckChaserPosition", aih_basicperp::abi_check_chaser_position),
    func(0x8005_c8a8, "AIHigh_BasicPerp::Clear", aih_basicperp::abi_clear),
    func_ret(0x8005_c8e4, "AIHigh_BasicPerp::AIHigh_BasicPerp", aih_basicperp::abi_construct),
    func(0x8005_c92c, "AIHigh_BasicPerp::~AIHigh_BasicPerp", aih_basicperp::abi_destruct),
    func_ret(0x8005_c954, "AIHigh_BasicCop::AIHigh_BasicCop", aih_basiccop::abi_construct),
    func(0x8005_c9dc, "AIHigh_BasicCop::CheckSpikeBelt", aih_basiccop::abi_check_spike_belt),
    func_ret(0x8005_caa4, "AIHigh_BasicCop::ShouldIPerformCutOffBlock", aih_basiccop::abi_should_i_perform_cut_off_block),
    func(0x8005_cc04, "Blockade_AddRoadFlare", aih_basiccop::abi_add_road_flare),
    func(0x8005_cc58, "Blockade_AddObject", aih_basiccop::abi_add_object),
    func(0x8005_cdd4, "PlacePointOnRoad", aih_basiccop::abi_place_point_on_road),
    func(0x8005_cf80, "AIHigh_BasicCop::SetupBlockadeElements", aih_basiccop::abi_setup_blockade_elements),
    func(0x8005_d218, "AIHigh_BasicCop::HandleBlockadeSpeech", aih_basiccop::abi_handle_blockade_speech),
    func(0x8005_d33c, "AIHigh_BasicCop::~AIHigh_BasicCop", aih_basiccop::abi_destruct),
    func_ret(0x8005_d35c, "AIHigh_BTC_Cop::AIHigh_BTC_Cop", aih_btccop::abi_cop_construct),
    func(0x8005_d39c, "AIHigh_BTC_Cop::AssignToPlayer", aih_btccop::abi_cop_assign_to_player),
    func_ret(0x8005_d40c, "AIHigh_BTC_Cop::GetCheckChasePosition", aih_btccop::abi_cop_get_check_chase_position),
    func_ret(0x8005_d4a8, "AIHigh_BTC_Cop::CheckForNewTarget", aih_btccop::abi_cop_check_for_new_target),
    func(0x8005_d600, "AIHigh_BTC_Cop::StartArrest", aih_btccop::abi_cop_start_arrest),
    func(0x8005_d670, "AIHigh_BTC_Cop::FinishArrest", aih_btccop::abi_cop_finish_arrest),
    func(0x8005_d6d0, "AIHigh_BTC_Cop::FalseArrest", aih_btccop::abi_cop_false_arrest),
    func(0x8005_d6ec, "AIHigh_BTC_Cop::FreezeAndEndChase", aihigh::abi_nothing),
    func(0x8005_d6f4, "AIHigh_BTC_Cop::HudOff", aih_btccop::abi_cop_hud_off),
    func_ret(0x8005_d724, "AIHigh_BTC_HumanCop::AIHigh_BTC_HumanCop", aih_btccop::abi_human_cop_construct),
    func_ret(0x8005_d940, "AIHigh_BTC_HumanCop::FindRandomBarrierFreeArea", aih_btccop::abi_find_random_barrier_free_area),
    func(0x8005_db10, "AIHigh_BTC_HumanCop::ReleaseAndStartChase", aih_btccop::abi_human_cop_release_and_start_chase),
    func(0x8005_db80, "AIHigh_BTC_HumanCop::FreezeAndEndChase", aih_btccop::abi_human_cop_freeze_and_end_chase),
    func(0x8005_dd54, "AIHigh_BTC_HumanCop::CheckConditionWithCop0", aih_btccop::abi_human_cop_check_condition_with_cop0),
    func(0x8005_de40, "AIHigh_BTC_HumanCop::NewStage", aih_btccop::abi_human_cop_new_stage),
    func(0x8005_e1b0, "AIHigh_BTC_HumanCop::UpdateAndCheckTimeLeft", aih_btccop::abi_human_cop_update_and_check_time_left),
    func(0x8005_e2c4, "AIHigh_BTC_HumanCop::UpdateFreezeModeAndPullOverMode", aih_btccop::abi_human_cop_update_freeze_mode_and_pull_over_mode),
    func(0x8005_e3e8, "AIHigh_BTC_HumanCop::RequestWingman", aih_btccop::abi_human_cop_request_wingman),
    func(0x8005_e504, "AIHigh_BTC_HumanCop::RequestBlockader", aih_btccop::abi_human_cop_request_blockader),
    func_ret(0x8005_e6dc, "AIHigh_BTC_HumanCop::CheckForWingmanRequest", aih_btccop::abi_human_cop_check_for_wingman_request),
    func_ret(0x8005_e700, "AIHigh_BTC_HumanCop::CheckForBlockaderRequest", aih_btccop::abi_human_cop_check_for_blockader_request),
    func(0x8005_e744, "AIHigh_BTC_HumanCop::UpdateWingmanRole", aih_btccop::abi_human_cop_update_wingman_role),
    func(0x8005_e790, "AIHigh_BTC_HumanCop::ClearTrafficToPurgatory", aih_btccop::abi_clear_traffic_to_purgatory),
    func(0x8005_e7e8, "AIHigh_BTC_HumanCop::ResetClearTrafficToPurgatory", aih_btccop::abi_reset_clear_traffic_to_purgatory),
    func(0x8005_e83c, "AIHigh_BTC_HumanCop::SetDesiredSpeed", aih_btccop::abi_human_cop_set_desired_speed),
    func(0x8005_e89c, "AIHigh_BTC_HumanCop::HighExecute", aih_btccop::abi_human_cop_high_execute),
    func(0x8005_ea74, "AIHigh_BTC_HumanCop::HudOn", aih_btccop::abi_human_cop_hud_on),
    func_ret(0x8005_eae0, "AIHigh_BTC_Wingman::AIHigh_BTC_Wingman", aih_btccop::abi_wingman_construct),
    func(0x8005_eb2c, "AIHigh_BTC_Wingman::HighExecute", aih_btccop::abi_wingman_high_execute),
    func_ret(0x8005_f5b8, "AIHigh_BTC_Wingman::CheckForActivation", aih_btccop::abi_wingman_check_for_activation),
    func_ret(0x8005_f6e8, "AIHigh_BTC_Wingman::UpdateFreezeModeAndPullOverMode", aih_btccop::abi_wingman_update_freeze_mode_and_pull_over_mode),
    func(0x8005_f728, "AIHigh_BTC_Wingman::SetupWingman", aih_btccop::abi_wingman_setup_wingman),
    func(0x8005_f864, "AIHigh_BTC_Wingman::SetupBlockader", aih_btccop::abi_wingman_setup_blockader),
    func(0x8005_fdb4, "AIHigh_BTC_Wingman::~AIHigh_BTC_Wingman", aih_btccop::abi_wingman_destruct),
    func(0x8005_fdd4, "AIHigh_BTC_HumanCop::~AIHigh_BTC_HumanCop", aih_btccop::abi_human_cop_destruct),
    func(0x8005_fdf4, "AIHigh_BTC_Cop::~AIHigh_BTC_Cop", aih_btccop::abi_cop_destruct),
    func(0x8005_fe14, "AIState_NonActive::Execute@AIH_BTCCOP", aihigh::abi_nothing),
    func(0x8005_fe1c, "AIState_NonActive::~AIState_NonActive@AIH_BTCCOP", aih_btccop::abi_non_active_destruct),
    func_ret(0x8005_fe68, "AIState_Base::TestForRelease@AIH_BTCCOP", aihigh::abi_state_test_for_release),
    func(0x8005_fe70, "AIState_Base::~AIState_Base@AIH_BTCCOP", aih_btccop::abi_state_base_destruct),
    func(0x8005_fea4, "AIHigh_BTC_Perp::ReleaseCops", aih_btcperp::abi_release_cops),
    func(0x8005_ff68, "AIHigh_BTC_Perp::HandleCops", aih_btcperp::abi_handle_cops),
    func_ret(0x8005_ff88, "AIHigh_BTC_Perp::IsFalseArrest", aih_btcperp::abi_is_false_arrest),
    func_ret(0x8006_01a8, "AIHigh_BTC_Perp::CheckForControlsPressed", aih_btcperp::abi_check_for_controls_pressed),
    func(0x8006_0248, "AIHigh_BTC_Perp::HandlePullOver", aih_btcperp::abi_handle_pull_over),
    func(0x8006_0420, "AIHigh_BTC_Perp::NotifyCopsOfArrest", aih_btcperp::abi_notify_cops_of_arrest),
    func(0x8006_04d8, "AIHigh_BTC_Perp::NotifyCopsOfArrestComplete", aih_btcperp::abi_notify_cops_of_arrest_complete),
    func(0x8006_0590, "AIHigh_BTC_Perp::NotifyCopsOfFalseArrest", aih_btcperp::abi_notify_cops_of_false_arrest),
    func(0x8006_0648, "AIHigh_BTC_Perp::NotifyHumanCopsOfArrestHud", aih_btcperp::abi_notify_human_cops_of_arrest_hud),
    func(0x8006_0700, "AIHigh_BTC_Perp::ClearForNewStage", aih_btcperp::abi_clear_for_new_stage),
    func_ret(0x8006_0764, "AIHigh_BTC_Perp::CheckForActivation", aih_btcperp::abi_check_for_activation),
    func(0x8006_086c, "AIHigh_BTC_HumanPerp::NewStage", aih_btcperp::abi_human_perp_new_stage),
    func(0x8006_0a8c, "AIHigh_BTC_HumanPerp::HighExecute", aih_btcperp::abi_human_perp_high_execute),
    func_ret(0x8006_0ae4, "AIHigh_BTC_AIPerp::AIHigh_BTC_AIPerp", aih_btcperp::abi_ai_perp_construct),
    func(0x8006_0b6c, "AIHigh_BTC_AIPerp::~AIHigh_BTC_AIPerp", aih_btcperp::abi_ai_perp_destruct),
    func(0x8006_0bbc, "AIHigh_BTC_AIPerp::AvoidCops", aih_btcperp::abi_avoid_cops),
    func(0x8006_0f00, "AIHigh_BTC_AIPerp::CalculateTimeTillContact", aih_btcperp::abi_calculate_time_till_contact),
    func(0x8006_0fa4, "AIHigh_BTC_AIPerp::FindClosestCop", aih_btcperp::abi_find_closest_cop),
    func(0x8006_10a4, "AIHigh_BTC_AIPerp::HighExecute", aih_btcperp::abi_ai_perp_high_execute),
    func(0x8006_1564, "AIHigh_BTC_AIPerp::NewStage", aih_btcperp::abi_ai_perp_new_stage),
    func(0x8006_1b10, "AIHigh_BTC_HumanPerp::~AIHigh_BTC_HumanPerp", aih_btcperp::abi_human_perp_destruct),
    func(0x8006_1b38, "AIHigh_BTC_Perp::~AIHigh_BTC_Perp@AIH_BTCPERP", aih_btcperp::abi_perp_destruct),
    func(0x8006_1b60, "AIState_NonActive::Execute@AIH_BTCPERP", aihigh::abi_nothing),
    func(0x8006_1b68, "AIState_NonActive::~AIState_NonActive@AIH_BTCPERP", aih_btcperp::abi_non_active_destruct),
    func_ret(0x8006_1bb4, "AIState_Base::TestForRelease@AIH_BTCPERP", aihigh::abi_state_test_for_release),
    func(0x8006_1bbc, "AIState_Base::~AIState_Base@AIH_BTCPERP", aih_btcperp::abi_state_base_destruct),
    func_ret(0x8006_1bf0, "AIHigh_Player::CheckIfABlockadeCanBeSetup", aih_play::abi_check_if_a_blockade_can_be_setup),
    func(0x8006_1f74, "AIHigh_Player::SetupBlockade", aih_play::abi_setup_blockade),
    func(0x8006_29fc, "AIHigh_Player::CheckForNewLevel", aih_play::abi_check_for_new_level),
    func(0x8006_2cdc, "AIHigh_Player::HandleSpeech", aih_play::abi_handle_speech),
    func(0x8006_2e28, "AIHigh_Player::MaintainAvailableCops", aih_play::abi_maintain_available_cops),
    func_ret(0x8006_30b0, "AIHigh_Player::AIHigh_Player", aih_play::abi_construct),
    func(0x8006_32b4, "AIHigh_Player::HandleCops", aih_play::abi_handle_cops),
    func(0x8006_3454, "AIHigh_Player::CleanupBlockaders", aih_play::abi_cleanup_blockaders),
    func(0x8006_356c, "AIHigh_Player::HandlePullOver", aih_play::abi_handle_pull_over),
    func(0x8006_3a38, "AIHigh_Player::~AIHigh_Player", aih_play::abi_destruct),
    func(0x8006_3a60, "AIHigh_Human::HighExecute", aih_hum::abi_high_execute),
    func_ret(0x8006_3a9c, "AIHigh_Human::AIHigh_Human", aih_hum::abi_construct),
    func(0x8006_3ad0, "AIHigh_Human::~AIHigh_Human", aih_hum::abi_destruct),
    func(0x8006_3af8, "AIHigh_Opponent::CheckForWipeOut", aih_opp::abi_check_for_wipe_out),
    func_ret(0x8006_3cd8, "AIHigh_Opponent::DoRearEnder", aih_opp::abi_do_rear_ender),
    func(0x8006_3fac, "AIHigh_Opponent::HighExecute", aih_opp::abi_high_execute),
    func_ret(0x8006_42d8, "AIHigh_Opponent::AIHigh_Opponent", aih_opp::abi_construct),
    func_ret(0x8006_431c, "AIHigh_Opponent::DoProvokedAttack", aih_opp::abi_do_provoked_attack),
    func(0x8006_43c8, "AIHigh_Opponent::~AIHigh_Opponent", aih_opp::abi_destruct),
    func_ret(0x8006_43f0, "AIHigh_Cop::AIHigh_Cop", aih_cop::abi_construct),
    func(0x8006_443c, "AIHigh_Cop::SetTuningLevers", aih_cop::abi_set_tuning_levers),
    func(0x8006_44b4, "AIHigh_Cop::HighExecute", aih_cop::abi_high_execute),
    func_ret(0x8006_5b94, "AIHigh_Cop::CheckForNeedyPlayers", aih_cop::abi_check_for_needy_players),
    func(0x8006_5c30, "AIHigh_Cop::CheckForWipeOut", aih_cop::abi_check_for_wipe_out),
    func_ret(0x8006_5da8, "AIHigh_Cop::CheckForNewTarget", aih_cop::abi_check_for_new_target),
    func(0x8006_6008, "AIHigh_Cop::AssignToPlayer", aih_cop::abi_assign_to_player),
    func_ret(0x8006_6094, "AIHigh_Cop::GetCheckChasePosition", aih_cop::abi_get_check_chase_position),
    func_ret(0x8006_6144, "AIHigh_Cop::CheckForNewTriggers", aih_cop::abi_check_for_new_triggers),
    func(0x8006_646c, "AIHigh_Cop::~AIHigh_Cop", aih_cop::abi_destruct),
    func_ret(0x8006_648c, "AIHigh_Traffic::CheckForCops", aih_traf::abi_check_for_cops),
    func_ret(0x8006_6560, "AIHigh_Traffic::CopCheck", aih_traf::abi_cop_check),
    func(0x8006_666c, "AIHigh_Traffic::HighExecute", aih_traf::abi_high_execute),
    func_ret(0x8006_6ef8, "AIHigh_Traffic::AIHigh_Traffic", aih_traf::abi_construct),
    func_ret(0x8006_6f38, "AIHigh_Traffic::CheckForNewTriggers", aih_traf::abi_check_for_new_triggers),
    func(0x8006_718c, "AIHigh_Traffic::~AIHigh_Traffic", aih_traf::abi_destruct),
    func(0x8006_71ac, "AICop_StartUp", aicop::abi_start_up),
    func(0x8006_7258, "AICop_Restart", aicop::abi_restart),
    func(0x8006_72a8, "AICop_CleanUp", aicop::abi_clean_up),
    func_ret(0x8006_7308, "AICop_NoCopsInArea", aicop::abi_no_cops_in_area),
    func(0x8006_73b0, "AIInit_StartUp1", aiinit::abi_start_up1),
    func(0x8006_7424, "AIInit_StartUp2", aiinit::abi_start_up2),
    func(0x8006_74fc, "AIInit_Reset1", aiinit::abi_reset1),
    func(0x8006_7508, "AIInit_Reset2", aiinit::abi_reset2),
    func(0x8006_760c, "AIInit_CleanUp1", aiinit::abi_clean_up1),
    func(0x8006_7644, "AIInit_CleanUp2", aiinit::abi_clean_up2),
    func(0x8006_770c, "AI_TrafficStartUp", aiinit::abi_traffic_start_up),
    func(0x8006_77a8, "AI_TrafficCleanUp", aiinit::abi_traffic_clean_up),
    func(0x8006_77f8, "AIInit_LoadConfigs", aiinit::abi_load_configs),
    func(0x8006_7854, "AIInit_LoadPhysicsConfig", aiinit::abi_load_physics_config),
    func(0x8006_7974, "AIInit_ClearAICar", aiinit::abi_clear_ai_car),
    func(0x8006_79ec, "AIInit_RestartAICar", aiinit::abi_restart_ai_car),
    func(0x8006_7b14, "AIInit_InitAICar", aiinit::abi_init_ai_car),
    func(0x8006_7ce8, "AIInit_DeInitAICar", aiinit::abi_de_init_ai_car),
    func(0x8006_7d68, "AIInit_InitAICar2", aiinit::abi_init_ai_car2),
    func(0x8006_7dd0, "AIInit_DeInitAICar2", aiinit::abi_nothing),
    func_ret(0x8006_7dd8, "AIInit_IsNonStandardCarFile", aiinit::abi_is_non_standard_car_file),
    func_ret(0x8006_7e50, "AILife_EvaluateLife", ailife::abi_evaluate_life),
    func(0x8006_7eb4, "AILife_RCSetSpeeds", ailife::abi_rc_set_speeds),
    func(0x8006_7ee4, "AILife_RCPickSliceAndDirection", ailife::abi_rc_pick_slice_and_direction),
    func(0x8006_831c, "AILife_RCPickDesiredLatPosition", ailife::abi_rc_pick_desired_lat_position),
    func(0x8006_8450, "AILife_PlaceCarAtLocation(6)", ailife::abi_place_car_at_location_at),
    func(0x8006_848c, "AILife_SetInitialSlicePositionOrientationEtc", ailife::abi_set_initial_slice_position_orientation_etc),
    func(0x8006_84e0, "AILife_PlaceCarAtLocation", ailife::abi_place_car_at_location),
    func(0x8006_86e4, "AILife_ReencarnateTraffic", ailife::abi_reencarnate_traffic),
    func(0x8006_8794, "AILife_ReencarnateTrafficByPosition", ailife::abi_reencarnate_traffic_by_position),
    func(0x8006_89a0, "AILife_ReencarnateCopBySlice", ailife::abi_reencarnate_cop_by_slice),
    func(0x8006_8b24, "AILife_ReencarnateCopByPosition", ailife::abi_reencarnate_cop_by_position),
    func(0x8006_8cd4, "AILife_ReencarnateCopByLatPosAndRotation", ailife::abi_reencarnate_cop_by_lat_pos_and_rotation),
    func_ret(0x8006_8e58, "AILife_IsCoordInThisLiveArea", ailife::abi_is_coord_in_this_live_area),
    func_ret(0x8006_8f04, "AILife_IsTrafficCarInAnyLiveArea", ailife::abi_is_traffic_car_in_any_live_area),
    func_ret(0x8006_8f88, "AILife_IsCoordInThisVisibleArea", ailife::abi_is_coord_in_this_visible_area),
    func_ret(0x8006_8fec, "AILife_IsCarInAnyVisibleArea", ailife::abi_is_car_in_any_visible_area),
    func_ret(0x8006_900c, "AILife_IsSliceInAnyVisibleArea", ailife::abi_is_slice_in_any_visible_area),
    func_ret(0x8006_90ac, "AILife_IsSliceCloseToAnyCopCar", ailife::abi_is_slice_close_to_any_cop_car),
    func_ret(0x8006_914c, "AILife_IsPositionInAnyVisibleArea", ailife::abi_is_position_in_any_visible_area),
    func(0x8006_91d0, "AILife_Debug", ailife::abi_debug),
    func(0x8006_91e8, "AIPerson_LoadGridAndSetPersonalityIndexes", aiperson::abi_load_grid_and_set_personality_indexes),
    func(0x8006_9234, "AIPerson_SetPersonality", aiperson::abi_set_personality),
    func(0x8006_92fc, "AIPerson_SetPersonalityPointers", aiperson::abi_set_personality_pointers),
    func(0x8006_9360, "AIPerson_LoadPersonalityData", aiperson::abi_load_personality_data),
    func(0x8006_96a4, "AIPerson_LoadScriptData", aiperson::abi_load_script_data),
    func(0x8006_9780, "AIPerson_LoadGlue", aiperson::abi_load_glue),
    func(0x8006_988c, "AIPerson_Startup", aiperson::abi_startup),
    func(0x8006_9a30, "AIPerson_Cleanup", aiperson::abi_cleanup),
    func(0x8006_9a38, "AIPhysic_StopCar", aiphysic::abi_stop_car),
    func(0x8006_9afc, "AIPhysic_RevEngine", aiphysic::abi_rev_engine),
    func(0x8006_9bcc, "AIPhysic_HandleShifting", aiphysic::abi_handle_shifting),
    func_ret(0x8006_9d34, "AIPhysic_CalculateGear", aiphysic::abi_calculate_gear),
    func_ret(0x8006_9e38, "AIPhysic_GearInvSpeedRange", aiphysic::abi_gear_inv_speed_range),
    func_ret(0x8006_9e60, "AIPhysic_GearTopSpeed", aiphysic::abi_gear_top_speed),
    func_ret(0x8006_9e88, "AIPhysic_GearBottomSpeed", aiphysic::abi_gear_bottom_speed),
    func_ret(0x8006_9eb0, "AIPhysic_CalcAcceleration", aiphysic::abi_calc_acceleration),
    func_ret(0x8006_a190, "AIPhysic_ModifyAccelerationAccordingToScript", aiphysic::abi_modify_acceleration_according_to_script),
    func_ret(0x8006_a1e8, "AIPhysic_CalcDeceleration", aiphysic::abi_calc_deceleration),
    func(0x8006_a234, "AIPhysic_CheckDesiredDirection", aiphysic::abi_check_desired_direction),
    func(0x8006_a27c, "AIPhysic_HandleSignalling", aiphysic::abi_handle_signalling),
    func(0x8006_a2f0, "AIPhysic_HandleWipeoutTimer", aiphysic::abi_handle_wipeout_timer),
    func(0x8006_a384, "AIPhysic_Main", aiphysic::abi_main),
    func(0x8006_a614, "AIPhysic_CheckForBadPosition", aiphysic::abi_check_for_bad_position),
    func_ret(0x8006_a6ec, "AIPhysics_UseCoolPhysics", aiphysic::abi_use_cool_physics),
    func(0x8006_a730, "AIPhysic_SimplePhysics", aiphysic::abi_simple_physics),
    func(0x8006_aa9c, "AIPhysic_SimplePhysics_LongVel", aiphysic::abi_simple_physics_long_vel),
    func(0x8006_adf4, "AIPhysic_SimplePhysics_LatVel", aiphysic::abi_simple_physics_lat_vel),
    func(0x8006_af18, "AIPhysic_CoolPhysics", aiphysic::abi_cool_physics),
    func(0x8006_afcc, "AIPhysic_HandleDirection", aiphysic::abi_handle_direction),
    func(0x8006_b0b4, "AIPhysic_Preperation", aiphysic::abi_preperation),
    func_ret(0x8006_b31c, "AIPhysic_CalculateRoadPosition", aiphysic::abi_calculate_road_position),
    func(0x8006_b43c, "AIPhysic_GetDesiredVector", aiphysic::abi_get_desired_vector),
    func(0x8006_ba24, "AIPhysic_TargetedGetDesiredVector", aiphysic::abi_targeted_get_desired_vector),
    func_ret(0x8006_ba60, "AIPhysic_CheckIfOutOfControl", aiphysic::abi_check_if_out_of_control),
    func(0x8006_bc00, "AIPhysic_OutOfControlPhysics", aiphysic::abi_out_of_control_physics),
    func_ret(0x8006_c270, "AIPhysic_GetRearEndDamageFactor", aiphysic::abi_get_rear_end_damage_factor),
    func(0x8006_c2c8, "AIPhysic_InControlPhysics", aiphysic::abi_in_control_physics),
    func(0x8006_cb7c, "AIPhysic_FinishUp", aiphysic::abi_finish_up),
    func(0x8006_cf68, "AIPhysic_CalculateRampedDesiredLatPos", aiphysic::abi_calculate_ramped_desired_lat_pos),
    func_ret(0x8006_d070, "AIPhysic_HitWallCheck", aiphysic::abi_hit_wall_check),
    func(0x8006_d120, "AIPhysic_ProcessBarrierCollision", aiphysic::abi_process_barrier_collision),
    func(0x8006_d174, "AIPhysic_ProcessCollision", aiphysic::abi_process_collision),
    func(0x8006_d1f0, "AIPhysic_ChangeDirection", aiphysic::abi_change_direction),
    func(0x8006_d2a8, "AIPhysic_CheckForGripReduction", aiphysic::abi_check_for_grip_reduction),
    func(0x8006_d43c, "AIPhysic_StartUp", aiphysic::abi_start_up),
    func(0x8006_d444, "AIPhysic_CleanUp", aiphysic::abi_clean_up),
    func(0x8006_d44c, "AIPhysic_Reset", aiphysic::abi_reset),
    func(0x8006_d454, "AIPhysic_ResetCar", aiphysic::abi_reset_car),
    func(0x8006_d460, "AIPhysic_InitCar", aiphysic::abi_init_car),
    func(0x8006_d5d4, "AIPhysic_DeInitCar", aiphysic::abi_de_init_car),
    func_ret(0x8006_d624, "AIDataRecord_t::AIDataRecord_t", aidatarecord::abi_record_construct),
    func(0x8006_d66c, "AIDataRecord_t::~AIDataRecord_t", aidatarecord::abi_record_destruct),
    func_ret(0x8006_d6c4, "AIDataRecord_t::AddRecordToCollection", aidatarecord::abi_add_record_to_collection),
    func_ret(0x8006_d700, "AIDataRecord_t::RemoveRecordFromCollection", aidatarecord::abi_remove_record_from_collection),
    func(0x8006_d73c, "AIDataRecord_t::StartUp1", aidatarecord::abi_start_up1),
    func(0x8006_d7a8, "AIDataRecord_t::StartUp2", aidatarecord::abi_start_up2),
    func(0x8006_d814, "AIDataRecord_t::CleanUp1", aidatarecord::abi_clean_up1),
    func(0x8006_d884, "AIDataRecord_t::CleanUp2", aidatarecord::abi_clean_up2),
    func(0x8006_d88c, "AIDataRecord_t::Setup", aidatarecord::abi_setup),
    func_ret(0x8006_d8dc, "AIDataRecord_t::Load", aidatarecord::abi_load),
    func_ret(0x8006_d920, "AIDataRecord_t::SaveAndPurge", aidatarecord::abi_save_and_purge),
    func_ret(0x8006_d970, "AIDataRecord_AccTable_t::AIDataRecord_AccTable_t", aidatarecord::abi_acc_table_construct),
    func_ret(0x8006_d9f4, "AIDataRecord_AccTable_t::Get", aidatarecord::abi_acc_table_get),
    func(0x8006_da0c, "AIDataRecord_AccTable_t::Setup", aidatarecord::abi_acc_table_setup),
    func_ret(0x8006_da78, "AIDataRecord_BestLine_t::AIDataRecord_BestLine_t", aidatarecord::abi_best_line_construct),
    func_ret(0x8006_db0c, "AIDataRecord_TrackCurve_t::AIDataRecord_TrackCurve_t", aidatarecord::abi_track_curve_construct),
    func_ret(0x8006_dba4, "AIDataRecord_TrackCurve_t::Get", aidatarecord::abi_track_curve_get),
    func_ret(0x8006_dbbc, "AIDataRecord_CurveSpeedTable_t::AIDataRecord_CurveSpeedTable_t", aidatarecord::abi_curve_speed_table_construct),
    func_ret(0x8006_dc4c, "AIDataRecord_CurveSpeedTable_t::Get", aidatarecord::abi_curve_speed_table_get),
    func(0x8006_dc80, "AIDataRecord_CurveSpeedTable_t::Upgrade", aidatarecord::abi_curve_speed_table_upgrade),
    func_ret(0x8006_dd0c, "AIDataRecord_CarTracking_t::Get", aidatarecord::abi_car_tracking_get),
    func(0x8006_dd14, "AIDataRecord_CarTracking_t::~AIDataRecord_CarTracking_t", aidatarecord::abi_car_tracking_destruct),
    func(0x8006_dd34, "AIDataRecord_CurveSpeedTable_t::~AIDataRecord_CurveSpeedTable_t", aidatarecord::abi_curve_speed_table_destruct),
    func(0x8006_dd54, "AIDataRecord_TrackCurve_t::~AIDataRecord_TrackCurve_t", aidatarecord::abi_track_curve_destruct),
    func(0x8006_dd74, "AIDataRecord_BestLine_t::~AIDataRecord_BestLine_t", aidatarecord::abi_best_line_destruct),
    func(0x8006_dd94, "AIDataRecord_AccTable_t::~AIDataRecord_AccTable_t", aidatarecord::abi_acc_table_destruct),
    func(0x8006_ddb4, "AISpeeds_StartUp", aispeeds::abi_start_up),
    func(0x8006_ddec, "AISpeeds_ReadTuningInfo", aispeeds::abi_read_tuning_info),
    func(0x8006_e070, "AISpeeds_CleanUp", aispeeds::abi_clean_up),
    func_ret(0x8006_e09c, "AISpeeds_SuperDuperSpeedUp", aispeeds::abi_super_duper_speed_up),
    func_ret(0x8006_e138, "AISpeeds_CalcOpponentTopSpeed", aispeeds::abi_calc_opponent_top_speed),
    func_ret(0x8006_e690, "AISpeeds_NeedToSlowDownForCurve", aispeeds::abi_need_to_slow_down_for_curve),
    func_ret(0x8006_e734, "AISpeeds_CalcOpponentCurveSpeed", aispeeds::abi_calc_opponent_curve_speed),
    func_ret(0x8006_e89c, "AISpeeds_BTCGetGlueFactor", aispeeds::abi_btc_get_glue_factor),
    func_ret(0x8006_ea58, "AISpeeds_GetNextAICar", aispeeds::abi_get_next_ai_car),
    func_ret(0x8006_ea64, "AISpeeds_GetPrevAICar", aispeeds::abi_get_prev_ai_car),
    func_ret(0x8006_ead0, "AISpeeds_GetCaravanFactor", aispeeds::abi_get_caravan_factor),
    func_ret(0x8006_ee8c, "AISpeeds_GetGlueFactor", aispeeds::abi_get_glue_factor),
    func_ret(0x8006_f098, "AISpeeds_GetDamageFactor", aispeeds::abi_get_damage_factor),
    func_ret(0x8006_f1b0, "AISpeeds_LimitGlueMultiplier", aispeeds::abi_limit_glue_multiplier),
    func_ret(0x8006_f2a4, "AISpeeds_CalcCopTopSpeed", aispeeds::abi_calc_cop_top_speed),
    func_ret(0x8006_f36c, "AISpeeds_CalcTrafficTopSpeed", aispeeds::abi_calc_traffic_top_speed),
    func_ret(0x8006_f50c, "AISpeeds_GetLegalSpeed", aispeeds::abi_get_legal_speed),
    func_ret(0x8006_f550, "AISpeeds_RandomizeTrafficSpeed", aispeeds::abi_randomize_traffic_speed),
    func(0x8006_f5dc, "AISpeeds_CalcDesiredSpeed", aispeeds::abi_calc_desired_speed),
    func_ret(0x8006_f680, "AISpeeds_CalculateOncomingCarSpeed", aispeeds::abi_calculate_oncoming_car_speed),
    func(0x8006_f728, "AISpeeds_SetTrafficSpeedRandomFactor", aispeeds::abi_set_traffic_speed_random_factor),
    func(0x8006_f7a4, "AISpeeds_MaintainLeaderBoard", aispeeds::abi_maintain_leader_board),
    func_ret(0x8006_f8a4, "AISpeeds_GetScriptFactor", aispeeds::abi_get_script_factor),
    func_ret(0x8006_f90c, "AISpeeds_GetUpgradeAccMult", aispeeds::abi_get_upgrade_acc_mult),
    func_ret(0x8006_f9c8, "AISpeeds_GetUpgradeBrakeMult", aispeeds::abi_get_upgrade_brake_mult),
    func_ret(0x8006_fa84, "AISpeeds_GetUpgradeHandlingMult", aispeeds::abi_get_upgrade_handling_mult),
    func_ret(0x8006_fb40, "AISpeeds_GetUpgradeTopSpeedMult", aispeeds::abi_get_upgrade_top_speed_mult),
    func_ret(0x8006_fbfc, "AISpeeds_CalcHumanTopSpeed", aispeeds::abi_calc_human_top_speed),
    func_ret(0x8006_fc1c, "AISpeeds_CalcHumanCurveSpeed", aispeeds::abi_calc_human_curve_speed),
    func(0x8006_fef8, "AIScript_Assign", aiscript::abi_assign),
    func(0x8006_ff18, "AIScript_ClearLastReactionIndex", aiscript::abi_clear_last_reaction_index),
    func(0x8006_ff3c, "AIScript_Startup", aiscript::abi_startup),
    func(0x8006_ff78, "AIScript_Cleanup", aiscript::abi_cleanup),
    func(0x8006_ff80, "AIScript_SubmitPlayerAction", aiscript::abi_submit_player_action),
    func(0x8007_0008, "AIScript_ProcessActionsAndReactions", aiscript::abi_process_actions_and_reactions),
    func_ret(0x8007_0170, "AIScript_DoReAction", aiscript::abi_do_re_action),
    func_ret(0x8007_01a0, "AIScript_GetReactionTicksLeft", aiscript::abi_get_reaction_ticks_left),
    func(0x8007_01bc, "AIState_StartUp", aistate::abi_start_up),
    func(0x8007_01fc, "AIState_Restart", aistate::abi_restart),
    func(0x8007_023c, "AIState_CleanUp", aistate::abi_nothing),
    func(0x8007_0244, "AIState_Base::StateExecute", aistate::abi_state_execute),
    func_ret(0x8007_02cc, "AIState_Base::AIState_Base", aistate::abi_base_construct),
    func_ret(0x8007_02e4, "AIState_Base::operator new", aistate::abi_pool_new),
    func(0x8007_0330, "AIState_Base::operator delete", aistate::abi_pool_delete),
    func(0x8007_036c, "AIState_Normal::Execute", aistate::abi_normal_execute),
    func_ret(0x8007_03bc, "AIState_Normal::AIState_Normal", aistate::abi_normal_construct),
    func(0x8007_0414, "AIState_Idle::Execute", aistate::abi_idle_execute),
    func(0x8007_04d0, "AIState_Idle::SetIdlePosition", aistate::abi_set_idle_position),
    func_ret(0x8007_04dc, "AIState_Chase::AIState_Chase", aistate::abi_chase_construct),
    func(0x8007_05e4, "AIState_Chase::~AIState_Chase", aistate::abi_chase_destruct),
    func(0x8007_066c, "AIState_Chase::SetTarget", aistate::abi_set_target),
    func(0x8007_074c, "AIState_Chase::SetMurderMode", aistate::abi_set_murder_mode),
    func(0x8007_0778, "AIState_Chase::SetUp", aistate::abi_set_up),
    func(0x8007_08d4, "AIState_Chase::DoSlowNitrous", aistate::abi_do_slow_nitrous),
    func(0x8007_08f0, "AIState_Chase::DoNitrous", aistate::abi_do_nitrous),
    func(0x8007_0a3c, "AIState_Chase::Execute", aistate::abi_chase_execute),
    func(0x8007_0bb0, "AIState_Chase::FarTargeting", aistate::abi_far_targeting),
    func_ret(0x8007_0c5c, "AIState_Chase::CalculateCloseTargettingAheadSlowDownFactor", aistate::abi_close_targeting_ahead_slow_down_factor),
    func(0x8007_0ce8, "AIState_Chase::CloseTargeting", aistate::abi_close_targeting),
    func(0x8007_1198, "AIState_Chase::ApproachTargeting", aistate::abi_approach_targeting),
    func(0x8007_14a4, "AIState_Chase::CheckForBarriersAndTargetAroundThem", aistate::abi_check_for_barriers_and_target_around_them),
    func_ret(0x8007_15f4, "AIState_Chase::FindBarrierEndSlice", aistate::abi_find_barrier_end_slice),
    func(0x8007_198c, "AIState_Offroad::~AIState_Offroad", aistate::abi_offroad_destruct),
    func_ret(0x8007_1a00, "AIState_Offroad::AIState_Offroad", aistate::abi_offroad_construct),
    func(0x8007_1b48, "AIState_Offroad::UnleashIfInRange", aistate::abi_unleash_if_in_range),
    func(0x8007_1bc0, "AIState_Offroad::Execute", aistate::abi_offroad_execute),
    func_ret(0x8007_1d6c, "AIState_Purgatory::AIState_Purgatory", aistate::abi_purgatory_construct),
    func(0x8007_1f1c, "AIState_Purgatory::~AIState_Purgatory", aistate::abi_purgatory_destruct),
    func_ret(0x8007_203c, "AIState_Purgatory::TestForRelease", aistate::abi_purgatory_test_for_release),
    func(0x8007_20b8, "AIState_Purgatory::Execute", aistate::abi_purgatory_execute),
    func(0x8007_2120, "AIState_Purgatory::StartUp", aistate::abi_purgatory_start_up),
    func_ret(0x8007_212c, "AIState_RovingTraffic::AIState_RovingTraffic", aistate::abi_roving_traffic_construct),
    func(0x8007_2194, "AIState_RovingTraffic::CheckIfCarIsNearbyAndStop", aistate::abi_check_if_car_is_nearby_and_stop),
    func(0x8007_2348, "AIState_RovingTraffic::Execute", aistate::abi_roving_traffic_execute),
    func(0x8007_26ec, "AIState_Donuts::~AIState_Donuts", aistate::abi_donuts_destruct),
    func(0x8007_2750, "AIState_Donuts::Execute", aistate::abi_donuts_execute),
    func_ret(0x8007_2c4c, "AIState_GotoSlice::AIState_GotoSlice", aistate::abi_goto_slice_construct),
    func(0x8007_2ca0, "AIState_GotoSlice::Execute", aistate::abi_goto_slice_execute),
    func_ret(0x8007_2db8, "AIState_GotoSlice::InTargetSliceRange", aistate::abi_in_target_slice_range),
    func_ret(0x8007_2dfc, "AIState_Cruise::AIState_Cruise", aistate::abi_cruise_construct),
    func(0x8007_2e68, "AIState_Cruise::Execute", aistate::abi_cruise_execute),
    func(0x8007_2f3c, "AIState_Cruise::~AIState_Cruise", aistate::abi_base_destruct),
    func(0x8007_2f70, "AIState_GotoSlice::~AIState_GotoSlice", aistate::abi_base_destruct),
    func_ret(0x8007_2fa4, "AIState_RovingTraffic::TestForRelease", aistate::abi_roving_traffic_test_for_release),
    func(0x8007_2fbc, "AIState_RovingTraffic::~AIState_RovingTraffic", aistate::abi_roving_traffic_destruct),
    func(0x8007_3030, "AIState_NonActive::Execute", aistate::abi_nothing),
    func(0x8007_3038, "AIState_NonActive::~AIState_NonActive", aistate::abi_non_active_destruct),
    func(0x8007_3084, "AIState_Idle::~AIState_Idle", aistate::abi_idle_destruct),
    func(0x8007_30dc, "AIState_Normal::~AIState_Normal", aistate::abi_base_destruct),
    func_ret(0x8007_3110, "AIState_Base::TestForRelease", aistate::abi_base_test_for_release),
    func(0x8007_3118, "AIState_Base::~AIState_Base", aistate::abi_base_destruct),
    func(0x8007_314c, "AITrigger_TriggerManager::Init", aitrigger::abi_init),
    func_ret(0x8007_3228, "AITrigger_TriggerManager::InsertTrigger", aitrigger::abi_insert_trigger),
    func_ret(0x8007_3324, "AITrigger_TriggerManager::GetNextTrigger", aitrigger::abi_get_next_trigger),
    func_ret(0x8007_3394, "AITrigger_TriggerManager::GetPrevTrigger", aitrigger::abi_get_prev_trigger),
    func_ret(0x8007_3404, "AITrigger_TriggerManager::CheckForTriggerAtSlice", aitrigger::abi_check_for_trigger_at_slice),
    func_ret(0x8007_35b8, "AITrigger_TriggerManager::GetTrigger", aitrigger::abi_get_trigger),
    func_ret(0x8007_3620, "AITrigger_TriggerManager::CheckForClosestTriggerOfType", aitrigger::abi_check_for_closest_trigger_of_type),
    func(0x8007_36f0, "AITrigger_TriggerManager::DescribeTrigger", aitrigger::abi_describe_trigger),
    func_ret(0x8007_36f8, "AITrigger_Compare", aitrigger::abi_compare),
    func(0x8007_3710, "AITrigger_TriggerManager::Sort", aitrigger::abi_sort),
    func_ret(0x8007_3748, "AITune_GetOneWay", aitune::abi_get_one_way),
    func_ret(0x8007_37a0, "AITune_GetDriveSide", aitune::abi_get_drive_side),
    func_ret(0x8007_37e0, "AITune_GetRoughLapTimes", aitune::abi_get_rough_lap_times),
    func(0x8007_3820, "AITune_StartUp1", aitune::abi_start_up1),
    func(0x8007_3858, "AITune_StartUp2", aitune::abi_nothing),
    func(0x8007_3860, "AITune_CleanUp1", aitune::abi_nothing),
    func(0x8007_3868, "AITune_CleanUp2", aitune::abi_nothing),
    func_ret(0x8007_3870, "AIWorld_ZSplineDistance", aiworld::abi_z_spline_distance),
    func_ret(0x8007_3904, "AIWorld_ZSplineDistance_Points", aiworld::abi_z_spline_distance_points),
    func_ret(0x8007_3998, "AIWorld_ApxSplineDistance", aiworld::abi_apx_spline_distance),
    func_ret(0x8007_3a08, "AIWorld_ApxSplineDistance_ToSlice", aiworld::abi_apx_spline_distance_to_slice),
    func_ret(0x8007_3a74, "AIWorld_ApxSplineDistance_FromSlice", aiworld::abi_apx_spline_distance_from_slice),
    func_ret(0x8007_3a9c, "AIWorld_ApxSplineDistance_Slices", aiworld::abi_apx_spline_distance_slices),
    func_ret(0x8007_3b04, "AIWorld_SplineDistance", aiworld::abi_spline_distance),
    func_ret(0x8007_3b5c, "AIWorld_SplineDistance_ToPoint", aiworld::abi_spline_distance_to_point),
    func_ret(0x8007_3bb8, "AIWorld_GameOdometer", aiworld::abi_game_odometer),
    func_ret(0x8007_3bd0, "AIWorld_IsDriveableLaneInSliceRange", aiworld::abi_is_driveable_lane_in_slice_range),
    func_ret(0x8007_3c88, "AIWorld_IsDriveableLane", aiworld::abi_is_driveable_lane),
    func_ret(0x8007_3cdc, "AIWorld_GetProfileMask", aiworld::abi_get_profile_mask),
    func_ret(0x8007_3d1c, "AIWorld_IsDriveableLane_UsingMask", aiworld::abi_is_driveable_lane_using_mask),
    func_ret(0x8007_3d38, "AIWorld_CheckForBarrierBetweenLanes", aiworld::abi_check_for_barrier_between_lanes),
    func_ret(0x8007_3dac, "AIWorld_LaneIndex", aiworld::abi_lane_index),
    func(0x8007_3e74, "AIWorld_CalculateLaneInfo", aiworld::abi_calculate_lane_info),
    func_ret(0x8007_3f38, "AIWorld_CalculateDeltaRoadYaw", aiworld::abi_calculate_delta_road_yaw),
    func_ret(0x8007_3fc0, "AIWorld_CalcRoadBend", aiworld::abi_calc_road_bend),
    func_ret(0x8007_409c, "AIWorld_CalcFutureLateralVel", aiworld::abi_calc_future_lateral_vel),
    func(0x8007_41b4, "AIWorld_CalcSpeed", aiworld::abi_calc_speed),
    func_ret(0x8007_41fc, "AIWorld_CalcLateralVelocity", aiworld::abi_calc_lateral_velocity),
    func(0x8007_4258, "AIWorld_FindBarrierLessLaneAndPosition", aiworld::abi_find_barrier_less_lane_and_position),
    func_ret(0x8007_df6c, "xzsquaredist32", bworld::abi_xzsquaredist32),
    func_ret(0x8007_f334, "BWorldSm_Init", bworldsm::abi_init),
    func(0x8007_f36c, "BWorldSm_Restart", bworldsm::abi_restart),
    func(0x8007_f38c, "BWorldSm_DeInit", bworldsm::abi_deinit),
    func(0x8007_f394, "FindAbsClosestSliceCrude", bworldsm::abi_find_abs_closest_slice_crude),
    func_ret(0x8007_f438, "BWorldSm_FindClosestSlice", bworldsm::abi_find_closest_slice),
    func(0x8007_f4d4, "RawFindClosestSlice", bworldsm::abi_raw_find_closest_slice),
    func(0x8007_f788, "BWorldSm_SetSlice", bworldsm::abi_set_slice),
    func(0x8007_f7dc, "GetStmQuadPts", bworldsm::abi_get_stm_quad_pts),
    func(0x8007_f904, "SetStrip", bworldsm::abi_set_strip),
    func(0x8007_f9bc, "GetFirstStmQuadPts", bworldsm::abi_get_first_stm_quad_pts),
    func(0x8007_fa58, "BWorld_SetSimSlice", bworldsm::abi_set_sim_slice),
    func(0x8007_fab8, "BworldSm_UpdateSimQuad", bworldsm::abi_update_sim_quad),
    func_ret(0x8007_fb40, "BworldSm_IsSimQuadValid", bworldsm::abi_is_sim_quad_valid),
    func_ret(0x8007_fb70, "RawFindClosestQuad", bworldsm::abi_raw_find_closest_quad),
    func_ret(0x8008_031c, "FindClosestQuad", bworldsm::abi_find_closest_quad),
    func_ret(0x8008_04e8, "BWorldSm_FindClosestQuadRez", bworldsm::abi_find_closest_quad_rez),
    func_ret(0x8008_06b4, "BWorldSm_FindClosestQuadMaxIterations", bworldsm::abi_find_closest_quad_max_iterations),
    func_ret(0x8008_06d4, "PointDirection", bworldsm::abi_point_direction),
    func_ret(0x8008_074c, "BWorldSm_FindEdgeOff", bworldsm::abi_find_edge_off),
    func_ret(0x8008_0868, "BWorldSm_QuadLight", bworldsm::abi_quad_light),
    func_ret(0x8008_09f8, "BWorldSm_TunnelFlagSm", bworldsm::abi_tunnel_flag_sm),
    func(0x8008_0a50, "NormalCache_AddEntry", bworldsm::abi_normal_cache_add_entry),
    func_ret(0x8008_0b0c, "NormalCache_FindEntry", bworldsm::abi_normal_cache_find_entry),
    func(0x8008_0bd0, "NormalCache_Init", bworldsm::abi_normal_cache_init),
    func(0x8008_0c0c, "Check_Rot", bworldsm::abi_check_rot),
    func_ret(0x8008_0f44, "BWorldSm_UNormal", bworldsm::abi_u_normal),
    func_ret(0x8008_0f6c, "BWorldSm_UForward", bworldsm::abi_u_forward),
    func_ret(0x8008_0f94, "BWorldSm_FindClosestTriangleRez", bworldsm::abi_find_closest_triangle_rez),
    func(0x8008_1030, "EulerToMat", camera::abi_euler_to_mat),
    func(0x8008_1128, "Camera_SetCamera", camera::abi_set_camera),
    func(0x8008_1160, "Camera_LookBack", camera::abi_look_back),
    func(0x8008_11e8, "Camera_TunnelLimit", camera::abi_tunnel_limit),
    func(0x8008_1330, "Camera_UpdateCollisionCam", camera::abi_update_collision_cam),
    func(0x8008_14ec, "Camera_UpdateSimpleCam", camera::abi_update_simple_cam),
    func(0x8008_15d0, "Camera_UpdateBumperCam", camera::abi_update_bumper_cam),
    func(0x8008_17a8, "Camera_UpdateTailCam", camera::abi_update_tail_cam),
    func(0x8008_1df0, "Camera_UpdateHeliCam", camera::abi_update_heli_cam),
    func(0x8008_24dc, "Camera_UpdateCircleCam", camera::abi_update_circle_cam),
    func(0x8008_275c, "SetCameraZoom", camera::abi_set_camera_zoom),
    func(0x8008_286c, "Camera_UpdateTVCam", camera::abi_update_tv_cam),
    func(0x8008_29b8, "Camera_UpdateAnimCam", camera::abi_update_anim_cam),
    func(0x8008_2c78, "Camera_UpdateFinishCam", camera::abi_update_finish_cam),
    func(0x8008_2da0, "Camera_UpdateBlimpCam", camera::abi_update_blimp_cam),
    func(0x8008_2ee4, "Camera_SetSplineCam", camera::abi_set_spline_cam),
    func(0x8008_30e4, "Camera_UpdateSplineCam", camera::abi_update_spline_cam),
    func_ret(0x8008_3660, "Camera_IslandProfile", camera::abi_island_profile),
    func(0x8008_36b8, "Camera_UpdatePulloverCam", camera::abi_update_pullover_cam),
    func(0x8008_3a34, "Camera_UpdateCopCam1", camera::abi_update_cop_cam1),
    func(0x8008_3bcc, "Camera_UpdateCopCam2", camera::abi_update_cop_cam2),
    func(0x8008_3cd4, "Camera_UpdateBTCopCam", camera::abi_update_bt_cop_cam),
    func(0x8008_3dfc, "Camera_Update", camera::abi_update),
    func(0x8008_427c, "Camera_Init", camera::abi_init),
    func(0x8008_4610, "Camera_Kill", camera::abi_kill),
    func(0x8008_4698, "Camera_PitchAndRoll", camera::abi_pitch_and_roll),
    func_ret(0x8008_474c, "Camera_TooSteep", camera::abi_too_steep),
    func(0x8008_48bc, "Camera_CheckWallCollisions", camera::abi_check_wall_collisions),
    func(0x8008_5230, "Camera_SetAboveGround", camera::abi_set_above_ground),
    func(0x8008_52bc, "Camera_AcquireTarget", camera::abi_acquire_target),
    func(0x8008_5658, "Camera_OpponentLookBehind", camera::abi_opponent_look_behind),
    func(0x8008_5a2c, "Camera_GetViewInfo", camera::abi_get_view_info),
    func(0x8008_5d60, "Camera_GetAudioViewInfo", camera::abi_get_audio_view_info),
    func_ret(0x8008_5f8c, "Camera_GetMode", camera::abi_get_mode),
    func(0x8008_602c, "Camera_SetMode", camera::abi_set_mode),
    func(0x8008_6240, "Camera_NextMode", camera::abi_next_mode),
    func(0x8008_65f4, "Camera_ReplayUpdate", camera::abi_replay_update),
    func(0x8008_66f8, "Camera_ResetRelPos", camera::abi_reset_rel_pos),
    func(0x8008_67a8, "Cars_DoGravityEffectsOnAcc", cars::abi_do_gravity_effects_on_acc),
    func(0x8008_68e8, "Cars_GetDashData", cars::abi_get_dash_data),
    func(0x8008_690c, "Cars_QDUpdateVelGlue", cars::abi_qd_update_vel_glue),
    func(0x8008_6988, "Cars_InitDashData", cars::abi_init_dash_data),
    func(0x8008_69b0, "Cars_SetAudioCalls", cars::abi_set_audio_calls),
    func(0x8008_6a78, "Cars_ResetCarCounters", cars::abi_reset_car_counters),
    func(0x8008_6aa0, "Cars_InitStats", cars::abi_init_stats),
    func(0x8008_6b08, "Cars_InitializeCarTablesFlagsAndCounters", cars::abi_initialize_car_tables_flags_and_counters),
    func(0x8008_7020, "Cars_ResetVariablesAfterACollision", cars::abi_reset_variables_after_a_collision),
    func(0x8008_7088, "Cars_ResetCollidedCars", cars::abi_reset_collided_cars),
    func(0x8008_74e8, "Cars_SetCarUpForHiRezSim", cars::abi_set_car_up_for_hi_rez_sim),
    func(0x8008_755c, "Cars_DoExtraCarCollisionProcessing", cars::abi_do_extra_car_collision_processing),
    func(0x8008_7eb0, "Cars_AddCarSfx", cars::abi_add_car_sfx),
    func(0x8008_82f0, "Car_TireSkiddingStuff", cars::abi_tire_skidding_stuff),
    func(0x8008_a184, "Cars_FindTotalSlice", cars::abi_find_total_slice),
    func(0x8008_a1f0, "Car_DoSkiddingStuff", cars::abi_do_skidding_stuff),
    func(0x8008_a300, "Car_DoPostCollisionStuff", cars::abi_do_post_collision_stuff),
    func(0x8008_a5d0, "Cars_CalculateStartingGridOffset", cars::abi_calculate_starting_grid_offset),
    func(0x8008_a814, "Cars_IniCarObjects", cars::abi_ini_car_objects),
    func(0x8008_ac00, "Cars_InitCar", cars::abi_init_car),
    func(0x8008_af10, "Cars_DeInitCar", cars::abi_de_init_car),
    func(0x8008_af58, "Cars_Restart", cars::abi_restart),
    func(0x8008_b040, "Cars_Initialize", cars::abi_initialize),
    func(0x8008_b064, "Cars_StartUp", cars::abi_start_up),
    func(0x8008_b4cc, "Cars_CleanUp", cars::abi_clean_up),
    func(0x8008_b654, "Cars_FindCurrentLap", cars::abi_find_current_lap),
    func_ret(0x8008_b70c, "Cars_CalculateRoadSpan", cars::abi_calculate_road_span),
    func_ret(0x8008_b954, "Cars_CalculateRoadPosition", cars::abi_calculate_road_position),
    func_ret(0x8008_b974, "Cars_CalcVelDownRoad", cars::abi_calc_vel_down_road),
    func(0x8008_ba10, "Cars_Randomize", cars::abi_randomize),
    func(0x8008_ba88, "Cars_ManageBureaucracy", cars::abi_manage_bureaucracy),
    func(0x8008_bc50, "Cars_CheckForAccidentScenes", cars::abi_check_for_accident_scenes),
    func(0x8008_bd18, "Cars_SortCars", cars::abi_sort_cars),
    func_ret(0x8008_c560, "CHECK_CENTER_VERTEX_WITH_DIRS", collide::abi_check_center_vertex_with_dirs),
    func_ret(0x8008_c9e0, "CHECK_CENTER_VERTEX_WITH_DIRS_OTHER", collide::abi_check_center_vertex_with_dirs_other),
    func_ret(0x8008_ce6c, "CHECK_VERTEX_WITH_DIRS", collide::abi_check_vertex_with_dirs),
    func_ret(0x8008_d234, "CHECK_VERTEX_WITH_DIRS_OTHER", collide::abi_check_vertex_with_dirs_other),
    func(0x8008_d5fc, "Collide_DoObjectFixedObjectCollision", collide::abi_do_object_fixed_object_collision),
    func(0x8008_e3a4, "Collide_LimitAngularVel", collide::abi_limit_angular_vel),
    func(0x8008_e498, "Collide_TestWithPlane", collide::abi_test_with_plane),
    func_ret(0x8008_f0c4, "Collide_DoObjectObjectCollision", collide::abi_do_object_object_collision),
    func_ret(0x8009_0040, "Collide_DoActualObjectCollisionCheck", collide::abi_do_actual_object_collision_check),
    func_ret(0x8009_0c34, "Collide_TestObjectVertices", collide::abi_test_object_vertices),
    func_ret(0x8009_1e64, "Collide_CheckForCollisionBetween", collide::abi_check_for_collision_between),
    func(0x8009_1fc4, "Collide_ClearCollisionRegistry", collide::abi_clear_collision_registry),
    func_ret(0x8009_21c4, "Collide_CheckAccuratePointRadiusCollision", collide::abi_check_accurate_point_radius_collision),
    func(0x8009_23bc, "Collide_CheckMeForCollisions", collide::abi_check_me_for_collisions),
    func(0x8009_2acc, "Control_AI", control::abi_control_ai),
    func(0x8009_2ad4, "Control_Human", control::abi_control_human),
    func_ret(0x8009_dea8, "Math_DistXZ", mathnfs::abi_dist_xz),
    func_ret(0x8009_defc, "Math_Dist3D", mathnfs::abi_dist_3d),
    func_ret(0x8009_df8c, "Math_BetterDist", mathnfs::abi_better_dist),
    func(0x8009_e0ec, "Math_ResolveRotatedVector", mathnfs::abi_resolve_rotated_vector),
    func_ret(0x8009_e1b0, "Math_VectorLength", mathnfs::abi_vector_length),
    func_ret(0x8009_e34c, "Math_VectorLength2", mathnfs::abi_vector_length2),
    func(0x8009_e4e0, "Math_NormalizeVector", mathnfs::abi_normalize_vector),
    func(0x8009_e698, "Math_NormalizeShortVector", mathnfs::abi_normalize_short_vector),
    func(0x8009_e744, "Math_QDNormalizeVector", mathnfs::abi_qd_normalize_vector),
    func(0x8009_e850, "Math_fasttransmult", mathnfs::abi_fasttransmult),
    func_ret(0x8009_e998, "tPauseMenuDefs::tPauseMenuDefs", mpause::abi_defs_ctor),
    func(0x8009_ecf8, "tPauseMenuDefs::~tPauseMenuDefs", mpause::abi_defs_dtor),
    func(0x8009_eeac, "MPause_MusicLogic", mpause::abi_music_logic),
    func(0x8009_f1ac, "MPause_ControllerLogic", mpause::abi_controller_logic),
    func_ret(0x8009_f290, "MPause_Logic", mpause::abi_logic),
    func(0x8009_f5ac, "MPause_Render", mpause::abi_render),
    func(0x8009_f754, "MPause_InitMPause", mpause::abi_init),
    func(0x8009_f78c, "MPause_StartPauseMenu", mpause::abi_start_pause_menu),
    func(0x8009_f9bc, "MPause_EndPauseMenu", mpause::abi_end_pause_menu),
    func(0x8009_f9f8, "MPause_KillMPause", mpause::abi_kill),
    func(0x8009_fa38, "Newton_AddDamageZone", newton::abi_add_damage_zone),
    func_ret(0x800a_0210, "Newton_FindClosestQuad", newton::abi_find_closest_quad),
    func_ret(0x800a_023c, "Newton_CalculateSliceYaw", newton::abi_calculate_slice_yaw),
    func(0x800a_02ac, "Newton_UpdateRoadGeometry", newton::abi_update_road_geometry),
    func_ret(0x800a_063c, "Newton_FindGroundElevationGeneral", newton::abi_find_ground_elevation_general),
    func_ret(0x800a_0798, "Newton_FindGroundElevationRough", newton::abi_find_ground_elevation_rough),
    func_ret(0x800a_096c, "Newton_FindGroundElevationAndNormalFast", newton::abi_find_ground_elevation_and_normal_fast),
    func_ret(0x800a_0a8c, "Newton_FindGroundElevationAndNormal", newton::abi_find_ground_elevation_and_normal),
    func(0x800a_18b0, "Newton_LimitCarsToDrivableDist", newton::abi_limit_cars_to_drivable_dist),
    func_ret(0x800a_18f4, "Newton_CalcPerpenHeightOfLowestPointFromGround", newton::abi_calc_perpen_height_of_lowest_point_from_ground),
    func_ret(0x800a_1c78, "Newton_CalcPerpenHeightOfCenterPointFromGround", newton::abi_calc_perpen_height_of_center_point_from_ground),
    func(0x800a_1d54, "Newton_CalcDistToClosestPlayerCar", newton::abi_calc_dist_to_closest_player_car),
    func(0x800a_2074, "Newton_UpdateRoadInfo", newton::abi_update_road_info),
    func(0x800a_2100, "Newton_CopyRoadMatrixToOrientMat", newton::abi_copy_road_matrix_to_orient_mat),
    func(0x800a_21d4, "Newton_CopyRoadMatrixToShadowMat", newton::abi_copy_road_matrix_to_shadow_mat),
    func(0x800a_22a8, "Newton_SetInitialSlicePositionOrientationEtc", newton::abi_set_initial_slice_position_orientation_etc),
    func(0x800a_26f4, "Newton_InitBaseNewtonObj", newton::abi_init_base_newton_obj),
    func(0x800a_2974, "Newton_QDUpdateVel", newton::abi_qd_update_vel),
    func_ret(0x800a_2a58, "Newton_OptzRotxform", newton::abi_optz_rotxform),
    func(0x800a_2bf4, "Newton_QDUpdateRot64Hz", newton::abi_qd_update_rot_64hz),
    func(0x800a_2d04, "Newton_QDUpdateRot32Hz", newton::abi_qd_update_rot_32hz),
    func(0x800a_2e24, "Newton_CalculateGroundShadowMatrix", newton::abi_calculate_ground_shadow_matrix),
    func(0x800a_3198, "Newton_CalcRealShadowCoordinates", newton::abi_calc_real_shadow_coordinates),
    func(0x800a_3538, "Newton_CheckForSpikeBelts", newton::abi_check_for_spike_belts),
    func(0x800a_35fc, "Newton_DoPostBarrierCollisionHandling", newton::abi_do_post_barrier_collision_handling),
    func(0x800a_37a4, "Newton_GenerateVector", newton::abi_generate_vector),
    func_ret(0x800a_3930, "Netwon_CheckForBadQuad", newton::abi_check_for_bad_quad),
    func(0x800a_3990, "Newton_TestForUndrivableSurfaces", newton::abi_test_for_undrivable_surfaces),
    func(0x800a_4140, "Newton_LimitAngularVelocity", newton::abi_limit_angular_velocity),
    func(0x800a_41d8, "Newton_ApplyTheLawOfGravity", newton::abi_apply_the_law_of_gravity),
    func_ret(0x800a_46c4, "Newton_CalculateRoadPositionFromSliceAndPosition", newton::abi_calculate_road_position_from_slice_and_position),
    func_ret(0x800a_47b8, "Newton_CalculateRoadPosition", newton::abi_calculate_road_position),
    func(0x800a_7af4, "PauseMenu_FullText", pausemenu::abi_full_text),
    func(0x800a_7ba8, "PauseMenu_MenuTextPositioned", pausemenu::abi_menu_text_positioned),
    func(0x800a_7c20, "PauseMenu_MenuText", pausemenu::abi_menu_text),
    func_ret(0x800a_7c84, "tPListIterator::tPListIterator", pausemenu::abi_iterator),
    func(0x800a_7ca0, "tPListIterator::~tPListIterator", pausemenu::abi_iterator_dtor),
    func_ret(0x800a_7cd4, "tPListIterator::Value", pausemenu::abi_value),
    func_ret(0x800a_7ce8, "tPListIterator::TextValue", pausemenu::abi_text_value),
    func(0x800a_7d38, "tPListIterator::Increment", pausemenu::abi_increment),
    func(0x800a_7da4, "tPListIterator::Decrement", pausemenu::abi_decrement),
    func_ret(0x800a_7e44, "tPListIteratorIndexed::tPListIteratorIndexed", pausemenu::abi_indexed),
    func(0x800a_7e88, "tPListIteratorIndexed::~tPListIteratorIndexed", pausemenu::abi_indexed_dtor),
    func_ret(0x800a_7eb0, "tPListIteratorIndexed::Value", pausemenu::abi_indexed_value),
    func_ret(0x800a_7ed4, "tPListIteratorIndexed::TextValue", pausemenu::abi_text_value),
    func(0x800a_7f24, "tPListIteratorIndexed::Increment", pausemenu::abi_indexed_increment),
    func(0x800a_7fb0, "tPListIteratorIndexed::Decrement", pausemenu::abi_indexed_decrement),
    func_ret(0x800a_807c, "tPMenuItem::tPMenuItem", pausemenu::abi_item),
    func(0x800a_8098, "tPMenuItem::~tPMenuItem", pausemenu::abi_item_dtor),
    func_ret(0x800a_80cc, "tPMenuItem::NextMenu", pausemenu::abi_item_next_menu),
    func_ret(0x800a_80d4, "tPMenuItem::Debounce", pausemenu::abi_item_debounce),
    func(0x800a_80dc, "tPMenuItem::ProcessInput", pausemenu::abi_item_process_input),
    func_ret(0x800a_80e4, "tPMenuItemNonInteractiveText::tPMenuItemNonInteractiveText", pausemenu::abi_text),
    func(0x800a_8118, "tPMenuItemNonInteractiveText::~tPMenuItemNonInteractiveText", pausemenu::abi_text_dtor),
    func(0x800a_8140, "tPMenuItemNonInteractiveText::Draw", pausemenu::abi_text_draw),
    func_ret(0x800a_8168, "tPMenuItemInteractive::tPMenuItemInteractive", pausemenu::abi_interactive),
    func(0x800a_819c, "tPMenuItemInteractive::~tPMenuItemInteractive", pausemenu::abi_interactive_dtor),
    func(0x800a_81c4, "tPMenuItemInteractive::Draw", pausemenu::abi_interactive_draw),
    func_ret(0x800a_81ec, "tPMenuItemLeftRightChoice::tPMenuItemLeftRightChoice", pausemenu::abi_choice),
    func(0x800a_8230, "tPMenuItemLeftRightChoice::~tPMenuItemLeftRightChoice", pausemenu::abi_choice_dtor),
    func(0x800a_8258, "tPMenuItemLeftRightChoice::ProcessInput", pausemenu::abi_choice_process_input),
    func(0x800a_82e4, "tPMenuItemLeftRightChoice::Draw", pausemenu::abi_choice_draw),
    func_ret(0x800a_84bc, "tPMenuItemLeftRightSlider::tPMenuItemLeftRightSlider", pausemenu::abi_slider),
    func(0x800a_8510, "tPMenuItemLeftRightSlider::~tPMenuItemLeftRightSlider", pausemenu::abi_slider_dtor),
    func_ret(0x800a_8538, "tPMenuItemLeftRightSlider::Debounce", pausemenu::abi_slider_debounce),
    func(0x800a_8540, "tPMenuItemLeftRightSlider::ProcessInput", pausemenu::abi_slider_process_input),
    func(0x800a_865c, "tPMenuItemLeftRightSlider::Draw", pausemenu::abi_slider_draw),
    func_ret(0x800a_8900, "tPMenuItemLeftRightSliderIndexed::tPMenuItemLeftRightSliderIndexed", pausemenu::abi_slider_indexed),
    func(0x800a_8948, "tPMenuItemLeftRightSliderIndexed::~tPMenuItemLeftRightSliderIndexed", pausemenu::abi_slider_indexed_dtor),
    func(0x800a_8970, "tPMenuItemLeftRightSliderIndexed::ProcessInput", pausemenu::abi_slider_indexed_process_input),
    func(0x800a_89bc, "tPMenuItemLeftRightSliderIndexed::Draw", pausemenu::abi_slider_indexed_draw),
    func_ret(0x800a_8a08, "tPMenuItemGoToMenuButton::tPMenuItemGoToMenuButton", pausemenu::abi_go_to_menu),
    func(0x800a_8a5c, "tPMenuItemGoToMenuButton::~tPMenuItemGoToMenuButton", pausemenu::abi_go_to_menu_dtor),
    func_ret(0x800a_8a84, "tPMenuItemGoToMenuButton::NextMenu", pausemenu::abi_go_to_menu_next_menu),
    func(0x800a_8a90, "tPMenuItemGoToMenuButton::ProcessInput", pausemenu::abi_go_to_menu_process_input),
    func_ret(0x800a_8b44, "tPMenuItemCommandButton::tPMenuItemCommandButton", pausemenu::abi_command_button),
    func(0x800a_8b88, "tPMenuItemCommandButton::~tPMenuItemCommandButton", pausemenu::abi_command_button_dtor),
    func(0x800a_8bb0, "tPMenuItemCommandButton::ProcessInput", pausemenu::abi_command_button_process_input),
    func(0x800a_8c10, "tPMenu::tPMenuConstructor", pausemenu::abi_menu_constructor),
    func_ret(0x800a_8c5c, "tPMenu::tPMenu", pausemenu::abi_menu),
    func(0x800a_8ca8, "tPMenu::~tPMenu", pausemenu::abi_menu_dtor),
    func(0x800a_8cdc, "tPMenu::Initialize", pausemenu::abi_initialize),
    func_ret(0x800a_8ddc, "tPMenu::Debounce", pausemenu::abi_debounce),
    func(0x800a_8e28, "tPMenu::CheckForDisabled", pausemenu::abi_check_for_disabled),
    func(0x800a_8f30, "tPMenu::ProcessInput", pausemenu::abi_process_input),
    func(0x800a_91a4, "tPMenu::Draw", pausemenu::abi_draw),
    func_ret(0x800a_9294, "tPMenu::NumEnabledItems", pausemenu::abi_num_enabled_items),
    func_ret(0x800a_92e4, "tPMenu::ItemEnabledNum", pausemenu::abi_item_enabled_num),
    func_ret(0x800a_932c, "tPMenuItemInteractive::IsNavigable", pausemenu::abi_interactive_is_navigable),
    func_ret(0x800a_9334, "tPMenuItemNonInteractiveText::IsNavigable", pausemenu::abi_text_is_navigable),
    func_ret(0x800a_933c, "tPMenuItem::IsEnabled", pausemenu::abi_is_enabled),
    func_ret(0x800a_9350, "tPMenuItem::IsDisabled", pausemenu::abi_is_disabled),
    func(0x800a_935c, "Physics_InitCarSpecs", physics::abi_init_car_specs),
    func(0x800a_95f8, "Physics_CalculateDerivedCarSpecs", physics::abi_calculate_derived_car_specs),
    func(0x800a_9b08, "Physics_CheckGamedata", physics::abi_check_gamedata),
    func_ret(0x800a_9b10, "Physics_AttenuateVelocity", physics::abi_attenuate_velocity),
    func(0x800a_9f6c, "Physics_SetCurrentWallType", physics::abi_set_current_wall_type),
    func_ret(0x800a_9f78, "Physics_GetTorque", physics::abi_get_torque),
    func(0x800a_9fb4, "Physics_CorrectPostCollisionYaw", physics::abi_correct_post_collision_yaw),
    func_ret(0x800a_a14c, "Physics_DoBarrierCheck", physics::abi_do_barrier_check),
    func(0x800a_a6e4, "Physics_AutoShift", physics::abi_auto_shift),
    func(0x800a_a9a0, "Physics_RampCarControlValues", physics::abi_ramp_car_control_values),
    func(0x800a_b178, "Physics_FixEngineRpm", physics::abi_fix_engine_rpm),
    func(0x800a_b2d0, "Physics_ResetCar", physics::abi_reset_car),
    func(0x800a_b36c, "Physics_StopCar", physics::abi_stop_car),
    func(0x800a_b444, "Physics_TestForBarrierCollision", physics::abi_test_for_barrier_collision),
    func(0x800a_b49c, "Physics_CalculateRoadGripModifiers", physics::abi_calculate_road_grip_modifiers),
    func_ret(0x800a_b618, "Physics_CalculateCarAcceleration", physics::abi_calculate_car_acceleration),
    func(0x800a_c130, "Physics_CalcWheelLockAcc", physics::abi_calc_wheel_lock_acc),
    func(0x800a_c32c, "Physics_CalcTractionCircleAcc", physics::abi_calc_traction_circle_acc),
    func(0x800a_c6d0, "Physics_CalculateTireForces", physics::abi_calculate_tire_forces),
    func_ret(0x800a_cc38, "Physics_CalculateRSControlDesiredPosition", physics::abi_calculate_rs_control_desired_position),
    func(0x800a_cdf8, "Physics_Real", physics::abi_real),
    func(0x800a_e1dc, "Physics_SimCar", physics::abi_sim_car),
    func_ret(0x800b_4920, "Replay_Compress", replay::abi_compress),
    func_ret(0x800b_49e4, "Replay_Decompress", replay::abi_decompress),
    func(0x800b_4a74, "Replay_InitReplay", replay::abi_init_replay),
    func(0x800b_4c18, "Replay_ResetReplay", replay::abi_reset_replay),
    func(0x800b_4d70, "Replay_StoringReplay", replay::abi_storing_replay),
    func(0x800b_4d90, "Replay_StoringControllerData", replay::abi_storing_controller_data),
    func_ret(0x800b_515c, "Replay_RetreivingControllerData", replay::abi_retreiving_controller_data),
    func(0x800b_54f8, "Replay_SaveInput", replay::abi_save_input),
    func(0x800b_580c, "Replay_GetInput", replay::abi_get_input),
    func(0x800b_5c6c, "Replay_SaveReplay", replay::abi_save_replay),
    func(0x800b_5cf8, "Replay_LoadReplay", replay::abi_load_replay),
    func(0x800b_5d00, "Replay_DoReplay", replay::abi_do_replay),
    func(0x800b_5d44, "Replay_GetInterfaceKey", replay::abi_get_interface_key),
    func(0x800b_61d4, "Replay_LoadCameraFile", replay::abi_load_camera_file),
    func(0x800b_6540, "Replay_ReplayChooseCamera", replay::abi_choose_camera),
    func(0x800b_67e8, "Replay_ReplayFindClosestCamera", replay::abi_find_closest_camera),
    func_ret(0x800b_6e90, "Sched_ExecuteCheck", schedule::abi_execute_check),
    func_ret(0x800b_6fc4, "Sched_CreateNewSchedule", schedule::abi_create_new_schedule),
    func(0x800b_7034, "Sched_CleanUpSchedule", schedule::abi_clean_up_schedule),
    func(0x800b_7054, "Sched_AddFunction", schedule::abi_add_function),
    func(0x800b_7108, "Sched_DeleteFunction", schedule::abi_delete_function),
    func(0x800b_71d4, "Sched_Execute", schedule::abi_execute),
    func(0x800b_7248, "Sim_StartUp", sim::abi_start_up),
    func(0x800b_735c, "Sim_Restart", sim::abi_restart),
    func(0x800b_7428, "Sim_CleanUp", sim::abi_clean_up),
    func(0x800b_74cc, "Sim_FadeInSFX", sim::abi_fade_in_sfx),
    func(0x800b_754c, "Sim_ProcessSimSchedules", sim::abi_process_sim_schedules),
    func(0x800b_7870, "Sim_ProcessPause", sim::abi_process_pause),
    func(0x800b_7974, "Sim_CheckForPause", sim::abi_check_for_pause),
    func(0x800b_7a20, "Sim_MainGameLoop", sim::abi_main_game_loop),
    func(0x800b_7fc0, "SimQueue_StartUp", simqueue::abi_start_up),
    func(0x800b_7fec, "SimQueue_Reset", simqueue::abi_reset),
    func(0x800b_808c, "SimQueue_CleanUp", simqueue::abi_clean_up),
    func_ret(0x800b_80a0, "SimQueue_Put", simqueue::abi_put),
    func(0x800b_81b8, "SimQueue_SetCurrentInput", simqueue::abi_set_current_input),
    func(0x800b_8278, "SimQueue_GetCurrentInput", simqueue::abi_get_current_input),
    func(0x800b_82a0, "SimQueue_SetLag", simqueue::abi_set_lag),
    func_ret(0x800b_82cc, "SimQueue_IsBlocking", simqueue::abi_is_blocking),
    func(0x800b_8c2c, "Stats_DoPlayerGlue", stats::abi_do_player_glue),
    func(0x800b_8de8, "Stats_ClearPosition", stats::abi_clear_position),
    func_ret(0x800b_8e28, "Stats_GetPosition", stats::abi_get_position),
    func_ret(0x800b_8e7c, "Stats_GetNumOpponents", stats::abi_get_num_opponents),
    func(0x800b_8ed0, "Stats_TrackStats", stats::abi_track_stats),
    func(0x800b_92d8, "Stats_ExtrapolateOpponentTimes", stats::abi_extrapolate_opponent_times),
    func(0x800b_9a80, "Stats_TrackEndGame", stats::abi_track_end_game),
    func(0x800d_27e0, "Hud_CreateHudViews", hud::abi_create_hud_views),
    func(0x800d_2acc, "Hud_GoTpage", hud::abi_go_tpage),
    func(0x800d_2b5c, "Hud_DebugInfo", hud::abi_nothing),
    func(0x800d_2b64, "Hud_DebugCrap", hud::abi_nothing),
    func(0x800d_2b6c, "Hud_BuildSprite", hud::abi_build_sprite),
    func(0x800d_2c18, "Hud_BuildSprite2", hud::abi_build_sprite2),
    func(0x800d_2c44, "Hud_FBuildSprite", hud::abi_f_build_sprite),
    func(0x800d_2cd8, "Hud_BuildSpriteFromFont", hud::abi_build_sprite_from_font),
    func(0x800d_2d98, "Hud_BuildF3", hud::abi_build_f3),
    func(0x800d_2e5c, "Hud_BuildGT4", hud::abi_build_gt4),
    func(0x800d_2f08, "Hud_BuildFT4", hud::abi_build_ft4),
    func(0x800d_2ff8, "Hud_BuildMirrorFT4", hud::abi_build_mirror_ft4),
    func(0x800d_30fc, "Hud_BuildMapMirrorFT4", hud::abi_build_map_mirror_ft4),
    func(0x800d_3208, "Hud_BuildF4", hud::abi_build_f4),
    func(0x800d_32ac, "Hud_BuildG4", hud::abi_build_g4),
    func(0x800d_3368, "Hud_BuildF4o", hud::abi_build_f4o),
    func(0x800d_343c, "Hud_FBuildGT4", hud::abi_f_build_gt4),
    func(0x800d_34c8, "Hud_FBuildFT4", hud::abi_f_build_ft4),
    func(0x800d_3558, "Hud_FBuildF4", hud::abi_f_build_f4),
    func(0x800d_3618, "uppercase", hud::abi_uppercase),
    func(0x800d_3678, "Hud_Kill", hud::abi_kill),
    func(0x800d_36c8, "Hud_Init0", hud::abi_init0),
    func(0x800d_3720, "Hud_InitMapFrame", hud::abi_init_map_frame),
    func(0x800d_3acc, "Hud_BuildTimeSprites", hud::abi_build_time_sprites),
    func(0x800d_3c00, "Hud_Init", hud::abi_init),
    func(0x800d_45c0, "Hud_InitTables", hud::abi_init_tables),
    func(0x800d_45ec, "Hud_BuildETimeString", hud::abi_build_e_time_string),
    func_ret(0x800d_4778, "Hud_BuildDistanceString", hud::abi_build_distance_string),
    func(0x800d_4918, "Hud_BuildTimeString", hud::abi_build_time_string),
    func(0x800d_4a68, "Hud_BuildTach", hud::abi_build_tach),
    func_ret(0x800d_4e9c, "Hud_BuildString", hud::abi_build_string),
    func(0x800d_51f8, "Hud_BuildNumbers0", hud::abi_build_numbers0),
    func(0x800d_5a44, "Hud_BuildNumbers", hud::abi_build_numbers),
    func(0x800d_661c, "Hud_InitMap", hud::abi_init_map),
    func(0x800d_66bc, "Hud_BuildMapMarkers", hud::abi_build_map_markers),
    func(0x800d_6b8c, "Hud_WingmanFlash", hud::abi_wingman_flash),
    func(0x800d_6c40, "Hud_BuildWingmanInterface", hud::abi_build_wingman_interface),
    func(0x800d_6f8c, "Hud_InitCdPlayer", hud::abi_init_cd_player),
    func(0x800d_6fb0, "Hud_BuildCdPlayer", hud::abi_build_cd_player),
    func_ret(0x800d_771c, "Hud_BuildRadar", hud::abi_build_radar),
    func(0x800d_7e24, "Hud_BuildReplay", hud::abi_build_replay),
    func_ret(0x800d_8120, "Hud_NextPlayer", hud::abi_next_player),
    func_ret(0x800d_8284, "Hud_NextPlayerNameOrCarOrTime", hud::abi_next_player_name),
    func(0x800d_840c, "Hud_RenderMapView", hud::abi_render_map_view),
    func(0x800d_8690, "Hud_BlackThinBox", hud::abi_black_thin_box),
    func(0x800d_887c, "Hud_Draw321Num", hud::abi_draw_321_num),
    func(0x800d_8a38, "Hud_Render321Go", hud::abi_render_321_go),
    func(0x800d_8b84, "Hud_BigBTCTime", hud::abi_big_btc_time),
    func(0x800d_8ea4, "Hud_RenderHudView", hud::abi_render_hud_view),
    func(0x800d_981c, "Hud_RenderTacView", hud::abi_render_tac_view),
    func_ret(0x800d_9938, "Hud_ParseTime", hud::abi_parse_time),
    func(0x800d_9abc, "Hud_RenderPauseBox", hud::abi_render_pause_box),
    func(0x800d_9c44, "Hud_Render", hud::abi_render),
    func(0x800d_a0ac, "Hud_PositionMap", hud::abi_position_map),
    func(0x800d_a1b4, "Hud_BTC_Update", hud::abi_btc_update),
    func(0x800d_a218, "Hud_BustedOverlayOn", hud::abi_busted_overlay_on),
    func(0x800d_a3d0, "Hud_BustedOverlayOff", hud::abi_busted_overlay_off),
    func(0x800d_a3dc, "Hud_Reset", hud::abi_reset),
    func(0x800d_a434, "Hud_BTC_QuitOut", hud::abi_btc_quit_out),
    func(0x800d_a4f8, "Hud_BTC_BonusTime", hud::abi_btc_bonus_time),
    func(0x800d_a510, "Hud_Perp_OverlayOn", hud::abi_perp_overlay_on),
    func(0x800d_a53c, "Hud_Perp_OverlayOff", hud::abi_perp_overlay_off),
    func_ret(0x800d_bf04, "Night_FindClosestColor", night::abi_find_closest_color),
    func(0x800d_bfcc, "Night_CreateNightTableElement", night::abi_create_night_table_element),
    func(0x800d_c190, "Night_CreateNightTable", night::abi_create_night_table),
    func(0x800d_c1b8, "Night_GenerateNextLightningEvent", night::abi_generate_next_lightning_event),
    func(0x800d_c22c, "Night_PauseLightningEffect", night::abi_pause_lightning_effect),
    func(0x800d_c2b0, "Night_DoLightningEffect", night::abi_do_lightning_effect),
    func(0x800d_c434, "Night_SetCopColor", night::abi_set_cop_color),
    func(0x800d_c4c8, "Night_InitPlayerHeadLightColor", night::abi_init_player_head_light_color),
    func(0x800d_c524, "Night_SetPlayerHeadLightColor", night::abi_set_player_head_light_color),
    func(0x800d_c560, "Night_InitCopLightColors", night::abi_init_cop_light_colors),
    func(0x800d_c5bc, "Night_SetCopLightColors", night::abi_set_cop_light_colors),
    func(0x800d_c63c, "Night_InitWeatherTables", night::abi_init_weather_tables),
    func(0x800d_c6c0, "Night_SetWeatherColors", night::abi_set_weather_colors),
    func(0x800d_c73c, "Night_GenerateAllLightTables", night::abi_generate_all_light_tables),
    func(0x800d_c9d0, "Night_InitNightDriving", night::abi_init_night_driving),
    func(0x800d_cb6c, "Night_KillNightDriving", night::abi_kill_night_driving),
    func(0x800d_cc0c, "Night_RestartNightDriving", night::abi_restart_night_driving),
    func(0x800d_cc94, "Night_SetEnviroment", night::abi_set_enviroment),
    func(0x800d_cda4, "Night_AdditiveNightCalc", night::abi_additive_night_calc),
    func(0x800d_dad0, "RPause_CopyBackToFrontBuffer", rpause::abi_copy_back_to_front_buffer),
    func(0x800d_db90, "RPause_StartPauseMenu", rpause::abi_start_pause_menu),
    func(0x800d_dbb0, "RPause_StopPauseMenu", rpause::abi_stop_pause_menu),
    func(0x800d_f300, "CalcStartSegment", skidmark::abi_calc_start_segment),
    func(0x800d_f4d0, "CalcOneSegment", skidmark::abi_calc_one_segment),
    func_ret(0x800d_f5f8, "Skidmark_CheckChunk", skidmark::abi_check_chunk),
    func(0x800d_f7a4, "Skidmark_Add", skidmark::abi_add),
    func(0x800d_fb78, "Skidmark_AddStretch", skidmark::abi_add_stretch),
    func(0x800d_ff14, "Skidmark_Stretch", skidmark::abi_stretch),
    func(0x800d_ff94, "Skidmark_EndStretch", skidmark::abi_end_stretch),
    func(0x800e_0070, "Skidmark_OnyxBuildFacets", skidmark::abi_onyx_build_facets),
    func(0x800e_0110, "Skidmark_InitSkidmark", skidmark::abi_init_skidmark),
    func(0x800e_0184, "Skidmark_RestartSkidmark", skidmark::abi_restart_skidmark),
    func(0x800e_0198, "Skidmark_KillSkidmark", skidmark::abi_kill_skidmark),
    func(0x800e_01c0, "Texture_InitPaletteSharing", texture::abi_init_palette_sharing),
    func(0x800e_023c, "Texture_DeInitPaletteSharing", texture::abi_de_init_palette_sharing),
    func(0x800e_0284, "Texture_ResetPaletteSharing", texture::abi_reset_palette_sharing),
    func_ret(0x800e_0294, "Texture_CheckForSharedPalette", texture::abi_check_for_shared_palette),
    func(0x800e_03c8, "Texture_AddSharedPalette", texture::abi_add_shared_palette),
    func(0x800e_0440, "Texture_InitClut", texture::abi_init_clut),
    func(0x800e_0544, "Texture_GetClutId", texture::abi_get_clut_id),
    func(0x800e_05b0, "Texture_MenuReleaseClutId", texture::abi_menu_release_clut_id),
    func(0x800e_0610, "Texture_ColorCarPalette", texture::abi_color_car_palette),
    func(0x800e_0648, "Texture_CopyPalette", texture::abi_copy_palette),
    func(0x800e_06a8, "Texture_ProcessPaletteCopy", texture::abi_process_palette_copy),
    func(0x800e_0754, "Texture_LoadImage", texture::abi_load_image),
    func(0x800e_0798, "Texture_Vramf", texture::abi_vramf),
    func(0x800e_0944, "Texture_Vramcf", texture::abi_vramcf),
    func_ret(0x800e_0aac, "Texture_GetTranslucencyMode", texture::abi_get_translucency_mode),
    func(0x800e_0b24, "Texture_LoadPmx", texture::abi_load_pmx),
    func(0x800e_0f64, "Texture_CloneUVPmx", texture::abi_clone_uv_pmx),
    func(0x800e_10c4, "Texture_InitTrackTexture", texture::abi_init_track_texture),
    func(0x800e_111c, "Texture_InitMenuClut", texture::abi_init_menu_clut),
    func(0x800e_121c, "Texture_InitMenuTexture", texture::abi_init_menu_texture),
    func(0x800e_1244, "Texture_CleanupMenuTexture", texture::abi_cleanup_menu_texture),
    func(0x800e_12a4, "Texture_LoadMenuTexture", texture::abi_load_menu_texture),
    func(0x800e_1458, "Texture_KillTrackTexture", texture::abi_kill_track_texture),
    func(0x800e_1460, "Texture_KillMenuTexture", texture::abi_kill_menu_texture),
    func_ret(0x800e_1490, "TextureProcess_TransColorCheck", textureprocess::abi_trans_color_check),
    func(0x800e_14c4, "TextureProcess_ColorClut", textureprocess::abi_color_clut),
    func_ret(0x800e_16cc, "TextureProcess_DepthColorCluts", textureprocess::abi_depth_color_cluts),
    func_ret(0x800e_17a4, "Fog_CheckRange", textureprocess::abi_check_range),
    func_ret(0x800e_17f8, "Fog_FindKey", textureprocess::abi_find_key),
    func_ret(0x800e_188c, "Fog_AllocKey", textureprocess::abi_alloc_key),
    func(0x800e_18d8, "Fog_AddKey", textureprocess::abi_add_key),
    func(0x800e_198c, "Fog_Update", textureprocess::abi_update),
    func_ret(0x800e_1ad0, "Fog_MakeTrackPathName", textureprocess::abi_make_track_path_name),
    func_ret(0x800e_1b20, "Fog_ReadFogKeys", textureprocess::abi_read_fog_keys),
    func(0x800e_1c38, "Fog_InitFogTriggers", textureprocess::abi_init_fog_triggers),
    func(0x800e_1d1c, "Fog_DeInitFogTriggers", textureprocess::abi_de_init_fog_triggers),
    func(0x800e_1d40, "TextureProcess_Init", textureprocess::abi_init),
    func(0x800e_1d54, "CV_ProcessWorldColors_FINAL", textureprocess::abi_process_world_colors),
    func(0x800e_1f14, "CV_ColorTracks", textureprocess::abi_color_tracks),
    func(0x800e_211c, "CV_InitColorVertices", textureprocess::abi_init_color_vertices),
    func(0x800e_2538, "TrsProj_SetTransPrecision", trsproj::abi_set_trans_precision),
    func(0x800e_2544, "TrsProj_ResetTransPrecision", trsproj::abi_reset_trans_precision),
    func(0x800e_2554, "TrsProj_SetProjection", trsproj::abi_set_projection),
    func(0x800e_25d4, "TrsProj_SetMenuProjection", trsproj::abi_set_menu_projection),
    func(0x800e_2614, "TrsProj_SetViewTrsProjEnviro", trsproj::abi_set_view_trs_proj_enviro),
    func(0x800e_2668, "TrsProj_SetPsxMatrix", trsproj::abi_set_psx_matrix),
    func(0x800e_2758, "TrsProj_SetPsxTrans", trsproj::abi_set_psx_trans),
    func(0x800e_27a8, "TrsProj_SetPsxTransZero", trsproj::abi_set_psx_trans_zero),
    func(0x800e_27d8, "TrsProj_TransPt", trsproj::abi_trans_pt),
    func(0x800e_2864, "TrsProj_TransPtN16", trsproj::abi_trans_pt_n16),
    func(0x800e_28ec, "TrsProj_TransformProjectVertex", trsproj::abi_transform_project_vertex),
    func_ret(0x800e_29cc, "Weather_GetNumParticles", weather::abi_get_num_particles),
    func(0x800e_29dc, "Weather_SetMatrix", weather::abi_set_matrix),
    func(0x800e_2aa0, "Weather_SetIdentMatrix", weather::abi_set_ident_matrix),
    func(0x800e_2b24, "Weather_InitSnow", weather::abi_init_snow),
    func(0x800e_2b84, "Weather_InitRain", weather::abi_init_rain),
    func(0x800e_2be4, "Weather_InitSplats", weather::abi_init_splats),
    func_ret(0x800e_2cf8, "Weather_GetNewState", weather::abi_get_new_state),
    func(0x800e_2d38, "Weather_ChangeDensityState", weather::abi_change_density_state),
    func(0x800e_2dac, "Weather_ChangeIntensityState", weather::abi_change_intensity_state),
    func(0x800e_2e24, "Weather_ChangeDensityBasedOnTime", weather::abi_change_density_based_on_time),
    func(0x800e_2efc, "Weather_ChangeIntensityBasedOnTime", weather::abi_change_intensity_based_on_time),
    func(0x800e_2ff4, "Weather_InitStateControls", weather::abi_init_state_controls),
    func(0x800e_30c0, "Weather_Restart", weather::abi_restart),
    func(0x800e_313c, "Weather_Init", weather::abi_init),
    func(0x800e_3488, "Weather_DeInit", weather::abi_de_init),
    func(0x800e_351c, "Weather_TransformVertex", weather::abi_transform_vertex),
    func_ret(0x800e_35e0, "Weather_CheckAndResetParticles", weather::abi_check_and_reset_particles),
    func(0x800e_395c, "Weather_QuickReOrthogonalize", weather::abi_quick_re_orthogonalize),
    func(0x800e_3bdc, "Weather_ProcessParticles", weather::abi_process_particles),
    func(0x800e_3fc8, "Weather_CreateSnow", weather::abi_create_snow),
    func(0x800e_4178, "Weather_CreateRain", weather::abi_create_rain),
    func(0x800e_433c, "Weather_CreateSplat", weather::abi_create_splat),
    func(0x800e_4464, "Weather_DoSplats", weather::abi_do_splats),
    func(0x800e_4628, "Weather_DoWeather", weather::abi_do_weather),
    func(0x800e_493c, "Weather_BuildWeather", weather::abi_build_weather),
];

/// The index of the function at `addr` in [`FUNCS`].
pub fn find(addr: u32) -> Option<usize> {
    FUNCS.binary_search_by_key(&addr, |f| f.addr).ok()
}

/// The index of the function called `name`.
pub fn find_name(name: &str) -> Option<usize> {
    FUNCS.iter().position(|f| f.name == name)
}

static ENABLED: OnceLock<AtomicBool> = OnceLock::new();

fn enabled_flag() -> &'static AtomicBool {
    ENABLED.get_or_init(|| AtomicBool::new(std::env::var("NFSHS_DECOMP").map_or(true, |v| v != "off" && v != "0")))
}

/// Whether CPUs created from now on run the decompiled code (`NFSHS_DECOMP` sets the default).
pub fn enabled() -> bool {
    enabled_flag().load(Ordering::Relaxed)
}

/// Turn the decompiled code on or off for CPUs created from now on.
pub fn set_enabled(on: bool) {
    enabled_flag().store(on, Ordering::Relaxed);
}

/// The functions an environment variable names (comma-separated), as indices into [`FUNCS`];
/// words in `extra` are skipped.
pub(crate) fn named_in(var: &str, extra: &[&str]) -> Option<Vec<usize>> {
    let list = std::env::var(var).ok()?;
    let mut out = Vec::new();
    for name in list.split(',').map(str::trim).filter(|n| !n.is_empty() && !extra.contains(n)) {
        match find_name(name) {
            Some(i) => out.push(i),
            None => eprintln!("{var}: no decompiled function {name}"),
        }
    }
    Some(out)
}

/// `NFSHS_DECOMP_SCRAMBLE_ONLY=Name,...`: the scramble (see [`abi::scramble`]) limited to those
/// functions (for finding which one reads a leftover).
pub(crate) fn scramble_allowed(addr: u32) -> bool {
    static ONLY: OnceLock<Option<Vec<u32>>> = OnceLock::new();
    ONLY.get_or_init(|| named_in("NFSHS_DECOMP_SCRAMBLE_ONLY", &[]).map(|v| v.into_iter().map(|i| FUNCS[i].addr).collect()))
        .as_ref()
        .is_none_or(|only| only.contains(&addr))
}

/// Per function: turned off by `NFSHS_DECOMP_OFF` or left out of `NFSHS_DECOMP_ONLY`.
fn turned_off() -> &'static [bool] {
    static OFF: OnceLock<Vec<bool>> = OnceLock::new();
    OFF.get_or_init(|| {
        let mut off = vec![false; FUNCS.len()];
        if let Some(only) = named_in("NFSHS_DECOMP_ONLY", &[]) {
            off = (0..FUNCS.len()).map(|i| !only.contains(&i)).collect();
        }
        for i in named_in("NFSHS_DECOMP_OFF", &[]).unwrap_or_default() {
            off[i] = true;
        }
        off
    })
}

/// Calls and checks of one function on this thread.
#[derive(Clone, Copy, Default, Debug)]
pub struct Stats {
    pub calls: u64,
    pub checks: u64,
    pub mismatches: u64,
}

struct Thread {
    /// The decompiled code steps aside (a check's reference run).
    suspended: Cell<bool>,
    stats: RefCell<Vec<Stats>>,
}

impl Drop for Thread {
    fn drop(&mut self) {
        if std::env::var_os("NFSHS_DECOMP_STATS").is_some() {
            print_stats(&self.stats.borrow());
        }
    }
}

thread_local! {
    static THREAD: Thread = Thread { suspended: Cell::new(false), stats: RefCell::new(vec![Stats::default(); FUNCS.len()]) };
}

pub(crate) fn set_suspended(on: bool) {
    THREAD.with(|t| t.suspended.set(on));
}

pub(crate) fn count(i: usize, f: impl FnOnce(&mut Stats)) {
    THREAD.with(|t| f(&mut t.stats.borrow_mut()[i]));
}

/// This thread's calls and checks per function (as [`FUNCS`]).
pub fn stats() -> Vec<Stats> {
    THREAD.with(|t| t.stats.borrow().clone())
}

pub fn print_stats(stats: &[Stats]) {
    eprintln!("decompiled functions (calls, checks, mismatches):");
    for (f, s) in FUNCS.iter().zip(stats) {
        eprintln!("  {:<44} {:>10} {:>10} {:>6}", f.name, s.calls, s.checks, s.mismatches);
    }
}

/// Route `c`'s calls to the decompiled functions (unless the decompiled code is turned off).
pub fn install(c: &mut Cpu) {
    if enabled() {
        c.decomp = decomp;
        c.dispatch = dispatch;
    }
}

/// [`Cpu::decomp`]: run the decompiled function at `addr`, if there is one and it is on.
pub fn decomp(c: &mut Cpu, addr: u32) -> bool {
    let Some(i) = find(addr) else { return false };
    if turned_off()[i] || THREAD.with(|t| t.suspended.get()) {
        return false;
    }
    count(i, |s| s.calls += 1);
    // the registers hold the caller's values, as the original's would
    abi::leave_prologue(c, FUNCS[i].addr, None, &[]);
    abi::watch::observe(c, "enter", FUNCS[i].addr);
    let scrambled = abi::scramble::scoped(FUNCS[i].addr);
    if scrambled {
        abi::scramble::on_entry(c);
    }
    if check::wanted(i) {
        check::run(c, i);
    } else {
        abi::on_stack(c, FUNCS[i].addr, FUNCS[i].run);
        if scrambled {
            abi::scramble::after_return(c);
        }
    }
    abi::watch::observe(c, "exit", FUNCS[i].addr);
    true
}

/// [`Cpu::dispatch`]: calls by address (function pointers) reach the decompiled code too.
pub fn dispatch(c: &mut Cpu, addr: u32) -> bool {
    decomp(c, addr) || nfs4rc::dispatch(c, addr)
}

#[cfg(test)]
mod tests {
    #[test]
    fn funcs_sorted() {
        assert!(super::FUNCS.windows(2).all(|w| w[0].addr < w[1].addr));
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use ps1rt::{Cpu, Mem};

    /// The game's gp (its small globals are gp-relative).
    pub const GP: u32 = 0x8013_D83C;

    /// Free RAM the tests put their structures in.
    pub const SCRATCH: u32 = 0x801F_0000;

    /// A CPU with NFS4.EXE loaded (its tables), or None when the game data is not there.
    pub fn cpu() -> Option<Cpu> {
        let exe = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../gamedata/NFS4.EXE");
        let Ok(exe) = std::fs::read(&exe) else {
            eprintln!("{} not found: skipped", exe.display());
            return None;
        };
        let mut m = Mem::default();
        m.load_exe(&exe);
        let mut c = Cpu::new(m);
        c.dispatch = nfs4rc::dispatch;
        c.dispatch_at = nfs4rc::dispatch_at;
        c.r[28] = GP;
        c.r[29] = 0x801F_FF00;
        Some(c)
    }

    /// Run a recompiled original with `args`; its v0.
    pub fn run(c: &mut Cpu, f: fn(&mut Cpu), args: &[u32]) -> u32 {
        crate::abi::call(c, f, args)
    }

    /// xorshift, with small and round numbers mixed in.
    pub struct Rng(pub u64);
    impl Rng {
        pub fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        pub fn any(&mut self) -> i32 {
            let v = self.next();
            let x = (v >> 32) as i32;
            match v & 7 {
                0 => x >> (v >> 8 & 31),
                1 => x & 0x1FFFF,
                2 => (x & 0xFF) - 0x80,
                _ => x,
            }
        }
    }

    /// A race's RAM (`BOOT_RAM=1 nfshs boot-race ...` writes it), or None.
    pub fn race_ram() -> Option<Vec<u8>> {
        boot_ram("ram.bin", "BOOT_RAM=1 nfshs boot-race")
    }

    /// The front end's RAM at the car select (`BOOT_RAM_FRAME=1850 nfshs boot-main` with the
    /// front-end tour's script up to the race writes it: one player, test drive, a track and
    /// cars browsed), or None.
    pub fn fe_ram() -> Option<Vec<u8>> {
        boot_ram("fe_ram.bin", "BOOT_RAM_FRAME=1850 nfshs boot-main")
    }

    fn boot_ram(file: &str, how: &str) -> Option<Vec<u8>> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../re/boot").join(file);
        let ram = std::fs::read(&path).ok();
        if ram.is_none() {
            eprintln!("{} not found ({how}): skipped", path.display());
        }
        ram
    }

    pub fn cpu_with(ram: &[u8]) -> Cpu {
        let mut m = Mem::default();
        m.ram.copy_from_slice(ram);
        let mut c = Cpu::new(m);
        c.dispatch = nfs4rc::dispatch;
        c.dispatch_at = nfs4rc::dispatch_at;
        c.inner_hook = Some(no_host);
        c.r[28] = GP;
        c.r[29] = 0x801F_FF00;
        c
    }

    /// What the videos answer in the tests: their state, a frame shown, a new video's handle.
    pub const VIDEO_ANSWERS: u32 = 0x8014_8AF0;

    /// The front end's shape emitter (0x8004e74c, under `DrawShapeExtended` and friends): in the
    /// game the host draws the shape itself and the original never runs (its code reads stack
    /// it never wrote: its results would hang on the callers' stack depth). The test CPU draws
    /// nothing either, but logs what the host gets, for the tests to compare: a count, a hash of
    /// every entry so far (a draw scrolled out of the ring still counts), then the last 16
    /// entries of 9 words (a ring): the shape, the flags, x, y, the four corners' colours, the
    /// blending.
    pub const SHAPES_EMITTED: u32 = 0x8014_8800;
    const SHAPE_EMITTER: u32 = 0x8004_E74C;
    /// `systemtask` and the ticks `timedwait` reads.
    const SYSTEMTASK: u32 = 0x800E_77F0;
    const TICKS: u32 = 0x8013_EF8C;

    fn emit_shape(c: &mut Cpu) {
        let sp = c.r[29];
        let (colours, abr) = (c.m.r32(sp + 16), c.m.r32(sp + 20));
        let n = c.m.r32(SHAPES_EMITTED);
        let entry = SHAPES_EMITTED + 8 + 36 * (n % 16);
        let mut words = vec![c.r[4], c.r[5], c.r[6], c.r[7]];
        words.extend((0..4).map(|k| c.m.r32(colours.wrapping_add(4 * k)) & 0xFF_FFFF));
        words.push(abr);
        let mut hash = c.m.r32(SHAPES_EMITTED + 4);
        for (k, w) in words.into_iter().enumerate() {
            c.m.w32(entry + 4 * k as u32, w);
            hash = (hash ^ w).wrapping_mul(0x0100_0193);
        }
        c.m.w32(SHAPES_EMITTED + 4, hash);
        c.m.w32(SHAPES_EMITTED, n.wrapping_add(1));
        c.r[2] = 0;
    }

    /// The game's host answers the library calls that aren't recompiled (pads, VSync, CD:
    /// from 0x800e0000 on, `src/ps1/host/system.rs`), and the race RAM's pointers into them
    /// are unset here; so is the pause menu's vtable when its race isn't paused. Such calls
    /// return 0 (`PadGetState` a stable pad, 6), for the original and the port alike; code
    /// that isn't recompiled below that (jumps into functions) is interpreted as usual, and so
    /// is the font's character reader (0x80107358, `currentfont`'s: reached only through its
    /// pointer, never recompiled; stubbed, every text would measure 0 pixels) and the tests' own
    /// routines among their structures (0x80140000 on). The front end's videos
    /// (`VIDEO_create` .. `VIDEO_updateframexy`) are the host's too: `VIDEO_state`,
    /// `VIDEO_updateframexy` and `VIDEO_create` answer what the tests put at [`VIDEO_ANSWERS`],
    /// the others 0. The front end's shape emitter is the host's as well (see [`SHAPES_EMITTED`]).
    fn no_host(c: &mut Cpu, addr: u32) -> bool {
        // `systemtask` lets a tick go by (`timedwait` waits for them, ticking it)
        if addr == SYSTEMTASK {
            let t = c.m.r32(TICKS).wrapping_add(1);
            c.m.w32(TICKS, t);
        }
        if addr == SHAPE_EMITTER {
            emit_shape(c);
            return true;
        }
        if (0x8005_11B0..=0x8005_1528).contains(&addr) {
            c.r[2] = match addr {
                0x8005_1480 => c.m.r32(VIDEO_ANSWERS),
                0x8005_1528 => c.m.r32(VIDEO_ANSWERS + 4),
                0x8005_11B0 => c.m.r32(VIDEO_ANSWERS + 8),
                _ => 0,
            };
            return true;
        }
        if addr != 0 &&(addr < 0x800E_0000 || addr == 0x8010_7358 || (0x8014_0000..0x8015_0000).contains(&addr)) {
            return false;
        }
        if addr != 0 && (c.dispatch)(c, addr) {
            if c.cont != 0 {
                c.run_cont();
            }
            return true;
        }
        c.r[2] = match addr {
            // `LoadImage` (the host's): the rectangle and its pixels hashed into LOADED_IMAGES
            0x800E_E3EC => {
                load_image(c);
                0
            }
            // `PadGetState`: what PAD_STATE holds, a stable pad (6) when it's 0
            0x800F_07FC => match c.m.r32(PAD_STATE) {
                0 => 6,
                state => state,
            },
            // `PadInfoMode`: the modes' count (a2 -1), else mode a2, from PAD_MODES
            0x800F_08BC if c.r[6] as i32 == -1 => c.m.r32(PAD_MODES),
            0x800F_08BC => c.m.r32(PAD_MODES + 4 + 4 * (c.r[6] % 7)),
            // libmcrd: `MemCardSync` its command and result through a1 and a2 too
            0x800F_C4A8 => {
                for (k, at) in [(5, 4), (6, 8)] {
                    if c.r[k] != 0 {
                        let v = c.m.r32(CARD_ANSWERS + at);
                        c.m.w32(c.r[k], v);
                    }
                }
                c.m.r32(CARD_ANSWERS)
            }
            0x800F_B770 => c.m.r32(CARD_ANSWERS + 12),
            0x800F_B534 => c.m.r32(CARD_ANSWERS + 16),
            0x800F_BDF4 => c.m.r32(CARD_ANSWERS + 20),
            0x800F_C014 => c.m.r32(CARD_ANSWERS + 24),
            0x800F_C5C4 => c.m.r32(CARD_ANSWERS + 28),
            0x800F_C7CC => c.m.r32(CARD_ANSWERS + 32),
            0x800F_C988 => c.m.r32(CARD_ANSWERS + 36),
            // `GetDirentry`: the directory (a2) filled from DIRECTORY when it is marked
            0x800F_C234 => {
                if c.m.r32(DIRECTORY) == DIRECTORY_MARK {
                    for k in 0..600 {
                        let v = c.m.r8(DIRECTORY + 4 + k);
                        c.m.w8(c.r[6] + k, v);
                    }
                }
                c.m.r32(CARD_ANSWERS + 40)
            }
            _ => 0,
        };
        true
    }

    /// Where the test CPUs answer libmcrd from: `MemCardSync`'s result, command and result,
    /// then `MemCardAccept`, `Exist`, `ReadFile`, `WriteFile`, `CreateFile`, `DeleteFile`,
    /// `Format`, `GetDirentry`.
    pub const CARD_ANSWERS: u32 = 0x8014_8A90;

    /// A directory `GetDirentry` answers with (600 bytes after the mark), when marked.
    pub const DIRECTORY: u32 = 0x8014_8C00;
    pub const DIRECTORY_MARK: u32 = 0x00D1_2E57;

    /// What the test CPUs' `LoadImage` was given (VRAM isn't RAM, so the checks would miss
    /// it): how many images, then a hash of each one's rectangle and pixels in turn.
    pub const LOADED_IMAGES: u32 = 0x8014_8AC4;

    fn load_image(c: &mut Cpu) {
        let (rect, data) = (c.r[4], c.r[5]);
        let (w, h) = (c.m.r16(rect.wrapping_add(4)) as u32, c.m.r16(rect.wrapping_add(6)) as u32);
        let mut hash = c.m.r32(LOADED_IMAGES + 4);
        let words = (0..2).map(|k| c.m.r32(rect.wrapping_add(4 * k))).collect::<Vec<_>>();
        let pixels = (0..(w * h).min(4096)).map(|k| c.m.r16(data.wrapping_add(2 * k)) as u32).collect::<Vec<_>>();
        for v in words.into_iter().chain(pixels) {
            hash = (hash ^ v).wrapping_mul(0x0100_0193);
        }
        c.m.w32(LOADED_IMAGES + 4, hash);
        let n = c.m.r32(LOADED_IMAGES);
        c.m.w32(LOADED_IMAGES, n.wrapping_add(1));
    }

    /// Where the test CPUs answer `PadInfoMode` from: the count, then seven modes.
    pub const PAD_MODES: u32 = 0x8014_8AD0;

    /// Where the test CPUs answer `PadGetState` from (0: a stable pad, 6).
    pub const PAD_STATE: u32 = 0x8014_8AC0;

    /// The words where two RAM images differ, but for the main stack (as `decomp-check`): the
    /// dead stack, and the callers' frames, where the original homes its arguments and the
    /// front end's rewritten code doesn't. (The stack tests check the race's fidelity there.)
    pub fn ram_diff(want: &[u8], got: &[u8]) -> Vec<usize> {
        let dead = 0x1F_0000..0x20_0000;
        (0..want.len()).step_by(4).filter(|a| !dead.contains(a) && want[*a..a + 4] != got[*a..a + 4]).collect()
    }

    /// `port` against the recompiled `original` (`args` its arguments) on the race's RAM as
    /// `shuffle` changes it: RAM, the rand() state and the result (when `port` returns one)
    /// must be the same. The port runs with the decompiled code routed, the original without.
    pub fn against_original(name: &str, cases: u32, seed: u64, shuffle: fn(&mut Cpu, &mut Rng), original: fn(&mut Cpu), args: &dyn Fn(&mut Cpu) -> Vec<u32>, port: fn(&mut Cpu, &[u32]) -> Option<u32>) {
        let Some(ram) = race_ram() else { return };
        against_original_in(&ram, name, cases, seed, shuffle, original, args, port);
    }

    /// [`against_original`] on another RAM image (`fe_ram`'s).
    #[allow(clippy::too_many_arguments)]
    pub fn against_original_in(ram: &[u8], name: &str, cases: u32, seed: u64, shuffle: fn(&mut Cpu, &mut Rng), original: fn(&mut Cpu), args: &dyn Fn(&mut Cpu) -> Vec<u32>, port: fn(&mut Cpu, &[u32]) -> Option<u32>) {
        let mut rng = Rng(seed);
        let mut c = cpu_with(ram);
        let plain = c.decomp;
        let (mut mismatches, mut busy) = (0, 0);
        for i in 0..cases {
            c.m.ram.copy_from_slice(ram);
            // (the image has no scratchpad: each case from an empty one, not the last's)
            c.m.scratch = [0; 0x400];
            shuffle(&mut c, &mut rng);
            let a = args(&mut c);
            let seed = rng.next() as u32;
            // (the scratchpad too: the front end keeps its drawing's state there)
            let (state, scratch) = (c.m.ram.clone(), c.m.scratch);
            c.rand_seed = seed;
            c.decomp = plain;
            let t0 = c.ticks;
            let want_v = run(&mut c, original, &a);
            let want_ticks = c.ticks - t0;
            let (want, want_seed) = (c.m.ram.clone(), c.rand_seed);
            c.m.ram.copy_from_slice(&state);
            c.m.scratch = scratch;
            c.rand_seed = seed;
            c.decomp = crate::decomp;
            let t0 = c.ticks;
            let got_v = port(&mut c, &a);
            let got_ticks = c.ticks - t0;
            busy += !ram_diff(&state, &want).is_empty() as u32;
            let diff = ram_diff(&want, &c.m.ram);
            let ticks_differ = COMPARE_TICKS.get() && got_ticks != want_ticks;
            if !diff.is_empty() || got_v.is_some_and(|v| v != want_v) || c.rand_seed != want_seed || ticks_differ {
                mismatches += 1;
                if mismatches <= 5 {
                    eprintln!("{name} case {i}: returned {got_v:x?}, the original {want_v:#x}; rand state {:#x}, the original {want_seed:#x}; {got_ticks} ticks, the original {want_ticks}", c.rand_seed);
                    report(name, i, &want, &c.m.ram, &diff);
                }
            }
        }
        eprintln!("{name}: {busy} of {cases} cases changed RAM");
        assert_eq!(mismatches, 0, "{name} differs from the original");
    }

    thread_local! {
        /// Whether [`against_original`] also compares the ticks each run takes (the loops'
        /// backward branches, which the host's spin detection counts as time while loading).
        pub static COMPARE_TICKS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// The scratchpad stack's top under a race's frames, for [`same_stack`].
    pub const SCRATCH_TOP: u32 = 0x1F80_03DC - 32;
    /// The dead stack in main RAM [`same_stack`] compares too.
    const DEAD: std::ops::Range<usize> = 0x1F_0000..0x1F_FF00;

    /// The function at `addr` entered from recompiled code (returning to `ra`, `args` in
    /// a0..a3, the stack at `top`: [`SCRATCH_TOP`] as in a race, or in main RAM for deep calls)
    /// on `shuffle`'s RAM leaves the stack (the scratchpad and main RAM's dead stack, filled
    /// with a random byte first) as the recompiled `original` does, the registers its callees
    /// save included, and the same RAM.
    #[allow(clippy::too_many_arguments)]
    pub fn same_stack(name: &str, addr: u32, original: fn(&mut Cpu), ra: u32, top: u32, cases: u32, seed: u64, shuffle: fn(&mut Cpu, &mut Rng), args: &dyn Fn(&mut Cpu, &mut Rng) -> Vec<u32>) {
        let Some(ram) = race_ram() else { return };
        same_stack_in(&ram, name, addr, original, ra, top, cases, seed, shuffle, args);
    }

    /// [`same_stack`] on another RAM image (`fe_ram`'s).
    #[allow(clippy::too_many_arguments)]
    pub fn same_stack_in(ram: &[u8], name: &str, addr: u32, original: fn(&mut Cpu), ra: u32, top: u32, cases: u32, seed: u64, shuffle: fn(&mut Cpu, &mut Rng), args: &dyn Fn(&mut Cpu, &mut Rng) -> Vec<u32>) {
        let mut rng = Rng(seed);
        let mut c = cpu_with(ram);
        let plain = c.decomp;
        let mut mismatches = 0;
        for i in 0..cases {
            c.m.ram.copy_from_slice(ram);
            shuffle(&mut c, &mut rng);
            let a = args(&mut c, &mut rng);
            let fill = rng.next() as u8;
            // (the seed from the case, not the generator: the cases stay those the tests had)
            let seed = (i ^ (fill as u32) << 16).wrapping_mul(0x9E37_79B9);
            let state = c.m.ram.clone();
            let regs = c.r;
            let enter = |c: &mut Cpu| {
                c.r = regs;
                c.rand_seed = seed;
                c.m.scratch = [fill; 0x400];
                c.m.ram[DEAD].fill(fill);
                for r in (16..24).chain([30]) {
                    c.r[r] = 0x8000_0000 | r as u32;
                }
                c.r[29] = top;
                // (a0..a3, then the stack's from the fifth on, as a caller's `jal` has them)
                let n = a.len().min(4);
                c.r[4..4 + n].copy_from_slice(&a[..n]);
                for (k, &v) in a.iter().enumerate().skip(4) {
                    c.m.w32(top + 4 * k as u32, v);
                }
                c.r[31] = ra;
            };
            enter(&mut c);
            let who = std::env::var("STACK_WHO").is_ok();
            if who {
                TRACE.with(|t| t.borrow_mut().clear());
                c.decomp = trace;
            }
            original(&mut c);
            // (an original the recompiler split ends by handing on its continuation)
            if c.cont != 0 {
                c.run_cont();
            }
            c.decomp = plain;
            let want = (c.m.scratch, c.m.ram.clone());
            let want_seed = c.rand_seed;
            c.m.ram.copy_from_slice(&state);
            enter(&mut c);
            crate::decomp(&mut c, addr);
            // (stack words by address)
            let word_at = |scratch: &[u8], ram: &[u8], at: u32| -> u32 {
                let (s, o) = if (0x1F80_0000..0x1F80_0400).contains(&at) { (scratch, (at - 0x1F80_0000) as usize) } else { (ram, (at & 0x1F_FFFF) as usize) };
                u32::from_le_bytes(s[o..o + 4].try_into().unwrap())
            };
            let stack = (0..0x400u32).step_by(4).map(|w| 0x1F80_0000 + w).chain(DEAD.step_by(4).map(|a| 0x8000_0000 | a as u32));
            let differ: Vec<u32> = stack.filter(|&at| word_at(&want.0, &want.1, at) != word_at(&c.m.scratch, &c.m.ram, at)).collect();
            let ram = ram_diff(&want.1, &c.m.ram);
            if !differ.is_empty() || !ram.is_empty() || c.rand_seed != want_seed {
                mismatches += 1;
                if mismatches <= 5 {
                    eprintln!("{name} case {i}: {} stack words differ; rand state {:#x}, the original {want_seed:#x}", differ.len(), c.rand_seed);
                    for &at in differ.iter().take(if who { 64 } else { 12 }) {
                        let by = if who { owner(&mut c, at) } else { String::new() };
                        let (w, g) = (word_at(&want.0, &want.1, at), word_at(&c.m.scratch, &c.m.ram, at));
                        eprintln!("  {at:#x}: expected {w:#010x} got {g:#010x} {by}");
                    }
                    report(name, i, &want.1, &c.m.ram, &ram);
                }
            }
        }
        assert_eq!(mismatches, 0, "{name} leaves the stack otherwise than the original");
    }

    thread_local! {
        /// The routed calls the original made in a [`same_stack`] case (`STACK_WHO`): the
        /// function and its stack pointer on entry.
        static TRACE: std::cell::RefCell<Vec<(u32, u32)>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    fn trace(c: &mut Cpu, addr: u32) -> bool {
        TRACE.with(|t| t.borrow_mut().push((addr, c.r[29])));
        false
    }

    /// The last traced function whose frame held the stack word at `at`, and where in it.
    fn owner(c: &mut Cpu, at: u32) -> String {
        TRACE.with(|t| {
            for &(f, sp) in t.borrow().iter().rev() {
                // (the prologue's `addiu sp, sp, -size`, not always the first instruction)
                let size = (0..8).map(|k| c.m.r32(f + 4 * k)).find(|w| w >> 16 == 0x27BD).map_or(0, |w| (w as u16 as i16 as i32).unsigned_abs());
                if at < sp && at >= sp - size {
                    let name = crate::find(f).map_or("?", |i| crate::FUNCS[i].name);
                    return format!("{name}+{}", at - (sp - size));
                }
            }
            "-".into()
        })
    }

    /// Print the first differing words of a failed case.
    pub fn report(name: &str, case: u32, want: &[u8], got: &[u8], diff: &[usize]) {
        eprintln!("{name} case {case}: {} words differ", diff.len());
        if let Ok(dir) = std::env::var("FUZZ_DUMP") {
            let _ = std::fs::write(format!("{dir}/{case}_want.bin"), want);
            let _ = std::fs::write(format!("{dir}/{case}_got.bin"), got);
        }
        for &a in diff.iter().take(10) {
            let w = |r: &[u8]| u32::from_le_bytes(r[a..a + 4].try_into().unwrap());
            let addr = 0x8000_0000 | a as u32;
            eprintln!("  {addr:#010x} {}: expected {:#x} got {:#x}", crate::check::describe_in(want, addr), w(want), w(got));
        }
    }
}
