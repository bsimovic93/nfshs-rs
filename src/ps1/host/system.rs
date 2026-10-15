//! The BIOS and system libraries, replaced: timers and VSync, callbacks, pads, events, CD,
//! streaming.

use super::*;

impl GameHost {
    /// The BIOS and system libraries: timers and VSync, callbacks, pads, events, CD, streaming. `None`: not one of these (or the original's own code runs).
    pub(super) fn system_call(&mut self, c: &mut Cpu, addr: u32) -> Option<u32> {
        let (a0, a1, a2, a3) = (c.r[4], c.r[5], c.r[6], c.r[7]);
        Some(match addr {
            // ---- libetc / timers / callbacks
            0x800F_2C78 => {
                // VSync(mode): mode 0 waits for the next field; mode 1 just reads the counter
                if a0 == 0 {
                    self.advance_to(c, (self.clock / FIELD + 1) * FIELD);
                }
                (self.clock / FIELD) as u32
            }
            0x800F_31A8 => 0, // ResetCallback
            0x800F_31D8 => {
                // InterruptCallback(line, handler): the previous handler
                self.callbacks.insert("interrupt", a1);
                let slot = c.irq_callbacks.get_mut(a0 as usize);
                slot.map_or(0, |h| std::mem::replace(h, a1))
            }
            0x800F_3208 => {
                // DMACallback(channel, handler): the previous handler
                self.callbacks.insert("dma", a1);
                let slot = c.dma_callbacks.get_mut(a0 as usize);
                slot.map_or(0, |h| std::mem::replace(h, a1))
            }
            0x800F_3238 => {
                let old = self.callbacks.insert("vsync", a0).unwrap_or(0);
                old
            }
            0x800E_A7CC | 0x800E_A8A0 => 1, // SetRCnt / StartRCnt
            0x800E_A868 => {
                // GetRCnt
                self.rcnt = self.rcnt.wrapping_add(64);
                self.rcnt & 0xFFFF
            }
            0x8010_7000 | 0x8010_7010 => 0, // ChangeClearPAD / ChangeClearRCnt
            0x800F_4D30 => 0,               // FlushCache
            0x8010_7318 | 0x8010_7338 => 1, // Exit/EnterCriticalSection

            // ---- pad
            0x800F_E6D4 => {
                // PadInitDirect(buf1, buf2)
                self.pad_buffers = [a0, a1];
                for b in [a0, a1] {
                    if b != 0 {
                        c.m.w8(b, 0xFF); // no controller until the host writes one
                    }
                }
                1
            }
            0x800F_07BC => 1, // PadStartCom
            0x800F_07FC => 6, // PadGetState: PadStateStable
            0x800F_08BC => u32::from(a1 == 1) * 4, // PadInfoMode: a digital pad (ID 4), no extended modes
            0x800F_0A88 => 1, // PadSetActAlign
            0x800F_0B08 => {
                // PadSetAct(port, data, len): libpad sends data[0..len] to the motors at every
                // VSync (the game's Force_Vbl / ShakeIt keep updating it)
                let port = ((a0 >> 4) & 1) as usize;
                self.actuators[port] = if a2 >= 2 { a1 } else { 0 };
                1
            }

            // ---- events (BIOS)
            0x8010_77CC => {
                // OpenEvent(desc, spec, mode, handler): RCNT2 interrupt = the game's 128 Hz timer
                if a0 == 0xF200_0002 && a3 != 0 && !c.timer_handlers.contains(&a3) {
                    c.timer_handlers.push(a3);
                    self.log.push(format!("timer handler {a3:#x}"));
                }
                let e = self.next_event;
                self.next_event += 1;
                e
            }
            0x8009_A5AC => {
                // CopSpeak_Stop waits for the cop speech queue (read gp+2932, write gp+2928)
                // to be played out: the requests still waiting are dropped
                let write = c.m.r32(0x8013_D83C + 2928);
                c.m.w32(0x8013_D83C + 2932, write);
                return None;
            }
            0x800E_77F0 => {
                if self.frame.on_frame.is_some() && c.r[31] == MAIN_LOOP_YIELD_RA {
                    // top of the original race loop
                    let _ = self.frame_top(c);
                } else {
                    // yield inside a wait loop: let time pass until the next timer tick
                    self.advance_to(c, (self.clock / TICK + 1) * TICK);
                    if let Ok(path) = std::env::var("PS1_STALL_DUMP") {
                        if self.clock > self.frame.last_top + 1920 * 30 {
                            std::fs::write(&path, &c.m.ram).ok();
                            eprintln!("stalled: RAM written to {path}; stack {:x?}", c.stack);
                            std::process::exit(3);
                        }
                    }
                }
                // then run the original systemtask
                return None;
            }
            0x8010_77BC | 0x8010_77DC | 0x8010_D4C0 | 0x8010_D354 => 1, // Enable/Disable/Close/Deliver
            0x8010_D4D0 => 1, // TestEvent: report every event as delivered

            // ---- CD (all game data goes through the FILE_* layer above; streaming is stubbed)
            0x8010_9CC8 => 1,                 // CdInit
            0x800F_81A8 | 0x8010_9924 => 0,   // CdSync / CdReadSync: complete
            0x800F_80DC => 0x02,              // CdStatus: motor on, idle
            0x800F_81C8 => 0,                 // CdReady
            0x800F_8210 | 0x800F_834C | 0x800F_8480 => 1, // CdControl*
            0x8010_9788 => 1,                 // CdRead
            0x8010_9A38 | 0x8010_9B74 => 2,   // CdDiskReady / CdGetDiskType (PS1 disc)
            0x8010_9ED8 | 0x8010_9EFC => 1,   // CdGetToc
            0x800F_80FC | 0x800F_8108 | 0x800F_8174 | 0x800F_8194 | 0x800F_85CC => 0,
            0x800F_81E8 | 0x800F_81FC | 0x800F_860C => 0, // Cd*Callback
            // ---- EA CD layer (only used for streaming once FILE_* is replaced)
            0x800F_AB04 | 0x800F_AB58 => 1,              // readsectorB / loaddirinfo
            0x800F_AD14 => 0,                            // CD_Restore
            0x800F_AD44 | 0x800F_AE58 => 1,              // CD_Init / CD_Restart
            0x800F_AF04 => 0,                            // CD_Open: streams unavailable
            0x800F_B00C | 0x800F_B2B0 | 0x800F_B2CC => 0, // CD_Close / CD_Stopread / CD_Getinfo
            0x800F_9954 => 0,                            // StSetStream

            // ---- streaming / MDEC / memory card: unavailable
            0x800F_92C4 | 0x800F_9824 | 0x800F_A354 => 0,
            0x800F_9324 | 0x800F_9358 | 0x800F_93F4 => 0,
            0x800F_B458 | 0x800F_B4A4 | 0x800F_B4F4 | 0x800F_C494 => 0,
            0x800F_C234 | 0x800F_C5C4 | 0x800F_C7CC | 0x800F_C988 => 0,
            0x800F_C4A8 => {
                // MemCardSync: report "no card"
                if a1 != 0 {
                    c.m.w32(a1, 0);
                }
                c.m.w32(a2, 0x02);
                1
            }
            _ => return None,
        })
    }
}
