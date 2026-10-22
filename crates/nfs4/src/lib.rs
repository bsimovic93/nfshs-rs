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
pub mod aiperson;
pub mod aiphysic;
pub mod aiscript;
pub mod aispeeds;
pub mod aitrigger;
pub mod aitune;
pub mod aiworld;
pub mod bworld;
pub mod bworldsm;
pub mod cars;
pub mod collide;
pub mod check;
pub mod control;
pub mod fixed;
pub mod globals;
pub mod mathnfs;
pub mod newton;
pub mod physics;
pub mod schedule;
pub mod sim;
pub mod simqueue;
pub mod stats;
pub mod types;
pub mod udff;
pub mod vec;

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
    func(0x8006_71ac, "AICop_StartUp", aicop::abi_start_up),
    func(0x8006_7258, "AICop_Restart", aicop::abi_restart),
    func(0x8006_72a8, "AICop_CleanUp", aicop::abi_clean_up),
    func_ret(0x8006_7308, "AICop_NoCopsInArea", aicop::abi_no_cops_in_area),
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
    if check::wanted(i) {
        check::run(c, i);
    } else {
        (FUNCS[i].run)(c);
    }
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
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../re/boot/ram.bin");
        let ram = std::fs::read(&path).ok();
        if ram.is_none() {
            eprintln!("{} not found (BOOT_RAM=1 nfshs boot-race): skipped", path.display());
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

    /// The game's host answers the library calls that aren't recompiled (pads, VSync, CD:
    /// from 0x800e0000 on, `src/ps1/host/system.rs`), and the race RAM's pointers into them
    /// are unset here; so is the pause menu's vtable when its race isn't paused. Such calls
    /// return 0 (`PadGetState` a stable pad, 6), for the original and the port alike; code
    /// that isn't recompiled below that (jumps into functions) is interpreted as usual.
    fn no_host(c: &mut Cpu, addr: u32) -> bool {
        if addr != 0 && addr < 0x800E_0000 {
            return false;
        }
        if addr != 0 && (c.dispatch)(c, addr) {
            if c.cont != 0 {
                c.run_cont();
            }
            return true;
        }
        c.r[2] = if addr == 0x800F_07FC { 6 } else { 0 };
        true
    }

    /// The words where two RAM images differ, but for the dead stack below 0x801FFF00.
    pub fn ram_diff(want: &[u8], got: &[u8]) -> Vec<usize> {
        let dead = 0x1F_0000..0x1F_FF00;
        (0..want.len()).step_by(4).filter(|a| !dead.contains(a) && want[*a..a + 4] != got[*a..a + 4]).collect()
    }

    /// `port` against the recompiled `original` (`args` its arguments) on the race's RAM as
    /// `shuffle` changes it: RAM, the rand() state and the result (when `port` returns one)
    /// must be the same. The port runs with the decompiled code routed, the original without.
    pub fn against_original(name: &str, cases: u32, seed: u64, shuffle: fn(&mut Cpu, &mut Rng), original: fn(&mut Cpu), args: &dyn Fn(&mut Cpu) -> Vec<u32>, port: fn(&mut Cpu, &[u32]) -> Option<u32>) {
        let Some(ram) = race_ram() else { return };
        let mut rng = Rng(seed);
        let mut c = cpu_with(&ram);
        let plain = c.decomp;
        let (mut mismatches, mut busy) = (0, 0);
        for i in 0..cases {
            c.m.ram.copy_from_slice(&ram);
            shuffle(&mut c, &mut rng);
            let a = args(&mut c);
            let seed = rng.next() as u32;
            let state = c.m.ram.clone();
            c.rand_seed = seed;
            c.decomp = plain;
            let want_v = run(&mut c, original, &a);
            let (want, want_seed) = (c.m.ram.clone(), c.rand_seed);
            c.m.ram.copy_from_slice(&state);
            c.rand_seed = seed;
            c.decomp = crate::decomp;
            let got_v = port(&mut c, &a);
            busy += !ram_diff(&state, &want).is_empty() as u32;
            let diff = ram_diff(&want, &c.m.ram);
            if !diff.is_empty() || got_v.is_some_and(|v| v != want_v) || c.rand_seed != want_seed {
                mismatches += 1;
                if mismatches <= 5 {
                    eprintln!("{name} case {i}: returned {got_v:x?}, the original {want_v:#x}; rand state {:#x}, the original {want_seed:#x}", c.rand_seed);
                    report(name, i, &want, &c.m.ram, &diff);
                }
            }
        }
        eprintln!("{name}: {busy} of {cases} cases changed RAM");
        assert_eq!(mismatches, 0, "{name} differs from the original");
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
        let mut rng = Rng(seed);
        let mut c = cpu_with(&ram);
        let plain = c.decomp;
        let mut mismatches = 0;
        for i in 0..cases {
            c.m.ram.copy_from_slice(&ram);
            shuffle(&mut c, &mut rng);
            let a = args(&mut c, &mut rng);
            let fill = rng.next() as u8;
            // (the seed from the case, not the generator: the cases stay those the tests had)
            let seed = (i ^ (fill as u32) << 16).wrapping_mul(0x9E37_79B9);
            let state = c.m.ram.clone();
            let enter = |c: &mut Cpu| {
                c.rand_seed = seed;
                c.m.scratch = [fill; 0x400];
                c.m.ram[DEAD].fill(fill);
                for r in (16..24).chain([30]) {
                    c.r[r] = 0x8000_0000 | r as u32;
                }
                c.r[29] = top;
                c.r[4..4 + a.len()].copy_from_slice(&a);
                c.r[31] = ra;
            };
            enter(&mut c);
            let who = std::env::var("STACK_WHO").is_ok();
            if who {
                TRACE.with(|t| t.borrow_mut().clear());
                c.decomp = trace;
            }
            original(&mut c);
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
        for &a in diff.iter().take(10) {
            let w = |r: &[u8]| u32::from_le_bytes(r[a..a + 4].try_into().unwrap());
            let addr = 0x8000_0000 | a as u32;
            eprintln!("  {addr:#010x} {}: expected {:#x} got {:#x}", crate::check::describe_in(want, addr), w(want), w(got));
        }
    }
}
