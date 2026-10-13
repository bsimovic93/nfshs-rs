//! The PS1 sound processor (SPU) and its DMA channel, modelled as the hardware the original
//! EA sound driver programs: 512 KB of sound RAM filled by DMA, 24 ADPCM voices with ADSR
//! envelopes, noise, pitch modulation, the reverb unit and the RAM-address interrupt the
//! driver's streams use to refill their buffers. Runs at 44.1 kHz of the virtual clock; the
//! mixed stereo samples go to [`crate::audio`].

use ps1rt::IoBus;

const RAM_SIZE: usize = 0x8_0000;
pub const RATE: u64 = 44_100;

const SPU_BASE: u32 = 0x1F80_1C00;
const DMA4_MADR: u32 = 0x1F80_10C0;
const DMA4_BCR: u32 = 0x1F80_10C4;
const DMA4_CHCR: u32 = 0x1F80_10C8;
const DPCR: u32 = 0x1F80_10F0;
const DICR: u32 = 0x1F80_10F4;

/// The ADPCM prediction filters (positive, negative coefficient / 64).
const FILTERS: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];

/// The SPU's 4-point Gaussian interpolation weights (by the pitch counter's fraction, bits
/// 4..11), a fixed table in the chip (psx-spx "SPU Interpolation").
#[rustfmt::skip]
const GAUSS: [i16; 512] = [
    -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 3, 3,
    3, 4, 4, 5, 5, 6, 7, 7, 8, 9, 9, 10, 11, 12, 13, 14,
    15, 16, 17, 18, 19, 21, 22, 24, 25, 27, 28, 30, 32, 33, 35, 37,
    39, 41, 44, 46, 48, 51, 53, 56, 58, 61, 64, 67, 70, 73, 77, 80,
    84, 87, 91, 95, 99, 103, 107, 111, 116, 120, 125, 130, 135, 140, 145, 150,
    156, 161, 167, 173, 179, 186, 192, 199, 205, 212, 219, 227, 234, 242, 250, 257,
    266, 274, 283, 291, 300, 309, 319, 328, 338, 348, 358, 369, 379, 390, 401, 412,
    424, 436, 448, 460, 473, 485, 498, 512, 525, 539, 553, 567, 582, 597, 612, 627,
    643, 659, 675, 692, 708, 726, 743, 761, 779, 797, 816, 835, 854, 874, 894, 914,
    935, 956, 977, 999, 1020, 1043, 1066, 1089, 1112, 1136, 1160, 1184, 1209, 1234, 1260, 1286,
    1312, 1339, 1366, 1394, 1422, 1450, 1479, 1508, 1537, 1567, 1598, 1628, 1660, 1691, 1723, 1756,
    1789, 1822, 1856, 1890, 1924, 1959, 1995, 2031, 2067, 2104, 2141, 2179, 2217, 2256, 2295, 2334,
    2374, 2415, 2456, 2497, 2539, 2582, 2624, 2668, 2712, 2756, 2801, 2846, 2892, 2938, 2985, 3032,
    3079, 3128, 3176, 3225, 3275, 3325, 3376, 3427, 3479, 3531, 3584, 3637, 3691, 3745, 3799, 3855,
    3910, 3967, 4023, 4081, 4138, 4197, 4255, 4315, 4374, 4435, 4495, 4557, 4619, 4681, 4744, 4807,
    4871, 4935, 5000, 5065, 5131, 5197, 5264, 5332, 5399, 5468, 5536, 5606, 5676, 5746, 5817, 5888,
    5959, 6032, 6104, 6177, 6251, 6325, 6400, 6475, 6550, 6626, 6702, 6779, 6856, 6934, 7012, 7091,
    7170, 7249, 7329, 7409, 7490, 7571, 7653, 7735, 7817, 7900, 7983, 8066, 8150, 8234, 8319, 8404,
    8489, 8575, 8661, 8748, 8834, 8922, 9009, 9097, 9185, 9273, 9362, 9451, 9541, 9630, 9720, 9811,
    9901, 9992, 10083, 10174, 10266, 10358, 10450, 10542, 10635, 10727, 10820, 10913, 11007, 11100, 11194, 11288,
    11382, 11476, 11571, 11665, 11760, 11855, 11950, 12045, 12140, 12236, 12331, 12427, 12522, 12618, 12714, 12809,
    12905, 13001, 13097, 13193, 13289, 13385, 13481, 13577, 13673, 13769, 13865, 13961, 14056, 14152, 14248, 14343,
    14439, 14534, 14630, 14725, 14820, 14915, 15010, 15104, 15199, 15293, 15387, 15481, 15575, 15669, 15762, 15855,
    15948, 16041, 16133, 16226, 16317, 16409, 16500, 16592, 16682, 16773, 16863, 16953, 17042, 17131, 17220, 17308,
    17396, 17484, 17571, 17658, 17744, 17830, 17916, 18001, 18086, 18170, 18254, 18337, 18420, 18502, 18584, 18665,
    18746, 18826, 18905, 18985, 19063, 19141, 19219, 19295, 19372, 19447, 19522, 19597, 19671, 19744, 19816, 19888,
    19959, 20030, 20100, 20169, 20238, 20306, 20373, 20439, 20505, 20570, 20634, 20698, 20760, 20822, 20884, 20944,
    21004, 21063, 21121, 21178, 21235, 21290, 21345, 21399, 21452, 21505, 21556, 21607, 21657, 21706, 21754, 21801,
    21848, 21893, 21938, 21982, 22025, 22066, 22107, 22148, 22187, 22225, 22262, 22299, 22334, 22369, 22402, 22435,
    22467, 22498, 22527, 22556, 22584, 22611, 22637, 22662, 22686, 22709, 22731, 22752, 22772, 22791, 22809, 22826,
    22842, 22857, 22872, 22885, 22897, 22908, 22918, 22927, 22935, 22942, 22948, 22953, 22957, 22960, 22962, 22963,
];

#[derive(Clone, Copy, Default, PartialEq, Debug)]
enum Phase {
    #[default]
    Off,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// One envelope step (ADSR or volume sweep): `rate` = shift << 2 | step, as the registers
/// hold it.
#[derive(Clone, Copy, Default)]
struct Envelope {
    counter: i32,
}

impl Envelope {
    fn tick(&mut self, level: i32, rate: u32, decreasing: bool, exponential: bool) -> i32 {
        let shift = (rate >> 2) as i32;
        let step = if decreasing { -8 + (rate & 3) as i32 } else { 7 - (rate & 3) as i32 };
        let mut cycles = 1i32 << (shift - 11).max(0);
        let mut step = step << (11 - shift).max(0);
        if exponential && !decreasing && level > 0x6000 {
            cycles *= 4;
        }
        if exponential && decreasing {
            step = step * level >> 15;
        }
        self.counter -= 1;
        if self.counter > 0 {
            return level;
        }
        self.counter = cycles;
        (level + step).clamp(0, 0x7FFF)
    }
}

#[derive(Clone, Default)]
struct Voice {
    /// The ADPCM block being played and the loop (repeat) address, bytes.
    addr: u32,
    repeat: u32,
    /// The repeat address was written while the voice played: blocks' loop-start flags no
    /// longer move it.
    ignore_loop: bool,
    /// Position in the block: 12-bit fraction, sample index above.
    counter: u32,
    /// The block's 28 samples after the previous block's last three (for interpolation).
    buf: [i16; 31],
    block_flags: u8,
    has_block: bool,
    /// ADPCM decoder history.
    s1: i32,
    s2: i32,
    phase: Phase,
    level: i32,
    env: Envelope,
    /// Output (after the envelope) of the last sample: modulates the next voice's pitch.
    last_out: i32,
    /// Current volumes (sweeps move them) and their envelopes.
    vol: [i32; 2],
    vol_env: [Envelope; 2],
}

pub struct Spu {
    pub ram: Vec<u8>,
    /// The registers as written (0x1F801C00..0x1F802000, 16-bit).
    regs: [u16; 0x200],
    voices: [Voice; 24],
    transfer_addr: u32,
    irq_flag: bool,
    endx: u32,
    noise_timer: i32,
    noise_level: u16,
    reverb_addr: u32,
    /// The reverb unit's current buffer position (bytes, mBASE..512K).
    reverb_pos: u32,
    reverb_in: [i32; 2],
    reverb_out: [i32; 2],
    odd: bool,
    main_vol: [i32; 2],
    main_env: [Envelope; 2],
    /// DMA channel 4 (MADR, BCR, CHCR), DPCR and DICR.
    dma4: [u32; 3],
    dpcr: u32,
    dicr: u32,
    irqs: u32,
    /// Mixed output (interleaved left, right) not yet taken.
    pub out: Vec<i16>,
    /// Keyed-on voices since the last [`Spu::take_keyons`] (diagnostics).
    pub keyons: u32,
}

impl Default for Spu {
    fn default() -> Self {
        Self {
            ram: vec![0; RAM_SIZE],
            regs: [0; 0x200],
            voices: Default::default(),
            transfer_addr: 0,
            irq_flag: false,
            endx: 0,
            noise_timer: 0,
            noise_level: 1,
            reverb_addr: 0,
            reverb_pos: 0,
            reverb_in: [0; 2],
            reverb_out: [0; 2],
            odd: false,
            main_vol: [0; 2],
            main_env: Default::default(),
            dma4: [0; 3],
            dpcr: 0x0765_4321,
            dicr: 0,
            irqs: 0,
            out: Vec::new(),
            keyons: 0,
        }
    }
}

fn sign15(v: u16) -> i32 {
    ((v as i32) << 17) >> 16
}

impl Spu {
    fn reg(&self, off: u32) -> u16 {
        self.regs[(off >> 1) as usize & 0x1FF]
    }
    fn spucnt(&self) -> u16 {
        self.reg(0x1AA)
    }

    /// Sound RAM touched at `addr..addr + len`: the IRQ address inside it raises the interrupt.
    fn check_irq(&mut self, addr: u32, len: u32) {
        if self.spucnt() & 0x40 == 0 || self.irq_flag {
            return;
        }
        let irq = (self.reg(0x1A4) as u32) << 3;
        if irq.wrapping_sub(addr) & (RAM_SIZE as u32 - 1) < len {
            self.irq_flag = true;
            self.irqs |= 1 << 9;
            if std::env::var_os("PS1_SNDLOG").is_some() {
                eprintln!("spu irq at {irq:#x}");
            }
        }
    }

    fn key_on(&mut self, v: usize) {
        let base = (v as u32) << 4;
        let start = (self.reg(base + 6) as u32) << 3;
        let voice = &mut self.voices[v];
        voice.addr = start;
        voice.counter = 0;
        voice.has_block = false;
        voice.ignore_loop = false;
        voice.s1 = 0;
        voice.s2 = 0;
        voice.buf = [0; 31];
        voice.level = 0;
        voice.env = Envelope::default();
        voice.phase = Phase::Attack;
        self.endx &= !(1 << v);
        self.keyons |= 1 << v;
    }

    fn key_off(&mut self, v: usize) {
        let voice = &mut self.voices[v];
        if voice.phase != Phase::Off {
            voice.phase = Phase::Release;
            voice.env = Envelope::default();
        }
    }

    fn write_reg(&mut self, off: u32, v: u16) {
        let off = off & 0x3FE;
        self.regs[(off >> 1) as usize] = v;
        match off {
            0x000..=0x17F => {
                let voice = (off >> 4) as usize;
                match off & 0xF {
                    // volumes: a fixed volume takes effect at once
                    0x0 | 0x2 => {
                        let side = ((off & 2) >> 1) as usize;
                        if v & 0x8000 == 0 {
                            self.voices[voice].vol[side] = sign15(v);
                        }
                    }
                    0xC => self.voices[voice].level = (v as i16 as i32).max(0),
                    0xE => {
                        let vc = &mut self.voices[voice];
                        vc.repeat = (v as u32) << 3;
                        vc.ignore_loop |= vc.phase != Phase::Off;
                    }
                    _ => {}
                }
            }
            0x180 | 0x182 => {
                if v & 0x8000 == 0 {
                    self.main_vol[((off & 2) >> 1) as usize] = sign15(v);
                }
            }
            0x188 | 0x18A => {
                let bits = (v as u32) << if off == 0x18A { 16 } else { 0 };
                for i in (0..24).filter(|i| bits >> i & 1 != 0) {
                    self.key_on(i);
                }
            }
            0x18C | 0x18E => {
                let bits = (v as u32) << if off == 0x18C { 0 } else { 16 };
                for i in (0..24).filter(|i| bits >> i & 1 != 0) {
                    self.key_off(i);
                }
            }
            0x19C | 0x19E => {}
            0x1A2 => {
                self.reverb_addr = (v as u32) << 3;
                self.reverb_pos = self.reverb_addr;
            }
            0x1A6 => self.transfer_addr = (v as u32) << 3,
            0x1A8 => {
                // manual write through the data FIFO
                let a = self.transfer_addr as usize & (RAM_SIZE - 2);
                self.ram[a..a + 2].copy_from_slice(&v.to_le_bytes());
                self.check_irq(a as u32, 2);
                self.transfer_addr = (self.transfer_addr + 2) & (RAM_SIZE as u32 - 1);
            }
            0x1AA => {
                if v & 0x40 == 0 {
                    self.irq_flag = false;
                }
            }
            _ => {}
        }
    }

    fn read_reg(&self, off: u32) -> u16 {
        let off = off & 0x3FE;
        match off {
            0x000..=0x17F if off & 0xF == 0xC => self.voices[(off >> 4) as usize].level as u16,
            0x000..=0x17F if off & 0xF == 0xE => (self.voices[(off >> 4) as usize].repeat >> 3) as u16,
            0x19C => self.endx as u16,
            0x19E => (self.endx >> 16) as u16,
            0x1A6 => (self.transfer_addr >> 3) as u16,
            0x1AE => {
                let cnt = self.spucnt();
                let dma = cnt >> 4 & 3;
                (cnt & 0x3F) | (self.irq_flag as u16) << 6 | ((dma >= 2) as u16) << 7 | ((dma == 2) as u16) << 8 | ((dma == 3) as u16) << 9
            }
            0x1B8 | 0x1BA => self.main_vol[((off & 2) >> 1) as usize] as u16,
            0x200..=0x25F => {
                let v = &self.voices[((off - 0x200) >> 2) as usize];
                v.vol[((off & 2) >> 1) as usize] as u16
            }
            _ => self.reg(off),
        }
    }

    /// DMA channel 4: `words` 32-bit words between main RAM at MADR and sound RAM at the
    /// transfer address.
    fn dma4(&mut self, ram: &mut [u8]) {
        let [madr, bcr, chcr] = self.dma4;
        let words = match chcr >> 9 & 3 {
            0 => match bcr & 0xFFFF {
                0 => 0x1_0000,
                n => n,
            },
            _ => (bcr & 0xFFFF) * (bcr >> 16),
        } as usize;
        let mut src = madr as usize & 0x1F_FFFC;
        let start = self.transfer_addr;
        for _ in 0..words * 4 {
            let a = self.transfer_addr as usize;
            if chcr & 1 != 0 {
                self.ram[a] = ram[src & (ram.len() - 1)];
            } else {
                ram[src & (ram.len() - 1)] = self.ram[a];
            }
            src += 1;
            self.transfer_addr = (self.transfer_addr + 1) & (RAM_SIZE as u32 - 1);
        }
        self.check_irq(start, (words * 4) as u32);
        if std::env::var_os("PS1_DMALOG").is_some() {
            eprintln!("dma4 {:#x} -> spu {start:#x}: {} bytes ({})", madr, words * 4, if chcr & 1 != 0 { "write" } else { "read" });
        }
        if chcr >> 9 & 3 == 1 {
            self.dma4[0] = src as u32;
            self.dma4[1] &= 0xFFFF;
        }
        self.dma4[2] &= !0x0100_0000;
        if self.dicr & (1 << 20) != 0 {
            self.dicr |= 1 << 28;
        }
        self.irqs |= 1 << 20;
    }

    /// The next ADPCM block of voice `v` into its buffer.
    fn decode_block(&mut self, v: usize) {
        let addr = self.voices[v].addr as usize & (RAM_SIZE - 16);
        self.check_irq(addr as u32, 16);
        let b = &self.ram[addr..addr + 16];
        let vc = &mut self.voices[v];
        let shift = match b[0] & 0xF {
            s @ 0..=12 => s as i32,
            _ => 9,
        };
        let (k0, k1) = FILTERS[((b[0] >> 4) as usize).min(4)];
        vc.block_flags = b[1];
        vc.buf.copy_within(28..31, 0);
        for i in 0..28 {
            let byte = b[2 + i / 2];
            let nib = if i & 1 == 0 { byte & 0xF } else { byte >> 4 } as i32;
            let raw = ((nib << 28) >> 28) << 12 >> shift;
            let s = (raw + ((vc.s1 * k0 + vc.s2 * k1 + 32) >> 6)).clamp(-32768, 32767);
            vc.s2 = vc.s1;
            vc.s1 = s;
            vc.buf[3 + i] = s as i16;
        }
        if vc.block_flags & 4 != 0 && !vc.ignore_loop {
            vc.repeat = addr as u32;
        }
        vc.has_block = true;
    }

    fn adsr(&mut self, v: usize) {
        let base = (v as u32) << 4;
        let (lo, hi) = (self.reg(base + 8) as u32, self.reg(base + 10) as u32);
        let vc = &mut self.voices[v];
        match vc.phase {
            Phase::Off => {}
            Phase::Attack => {
                vc.level = vc.env.tick(vc.level, lo >> 8 & 0x7F, false, lo & 0x8000 != 0);
                if vc.level >= 0x7FFF {
                    vc.phase = Phase::Decay;
                    vc.env = Envelope::default();
                }
            }
            Phase::Decay => {
                vc.level = vc.env.tick(vc.level, (lo >> 4 & 0xF) << 2, true, true);
                let sustain = (((lo & 0xF) + 1) * 0x800) as i32;
                if vc.level <= sustain {
                    vc.phase = Phase::Sustain;
                    vc.env = Envelope::default();
                }
            }
            Phase::Sustain => {
                vc.level = vc.env.tick(vc.level, hi >> 6 & 0x7F, hi & 0x4000 != 0, hi & 0x8000 != 0);
            }
            Phase::Release => {
                vc.level = vc.env.tick(vc.level, (hi & 0x1F) << 2, true, hi & 0x20 != 0);
                if vc.level == 0 {
                    vc.phase = Phase::Off;
                }
            }
        }
    }

    /// A volume register in sweep mode moves the current volume one step.
    fn sweep(env: &mut Envelope, cur: i32, reg: u16) -> i32 {
        if reg & 0x8000 == 0 {
            return sign15(reg);
        }
        let decreasing = reg & 0x2000 != 0;
        let exponential = reg & 0x4000 != 0;
        let negative = reg & 0x1000 != 0;
        let mag = if negative { -cur } else { cur }.clamp(0, 0x7FFF);
        let m = env.tick(mag, (reg & 0x7F) as u32, decreasing, exponential);
        if negative { -m } else { m }
    }

    fn noise_step(&mut self) {
        let cnt = self.spucnt();
        let shift = (cnt >> 10 & 0xF) as i32;
        let step = (cnt >> 8 & 3) as i32 + 4;
        self.noise_timer -= step;
        if self.noise_timer < 0 {
            let l = self.noise_level;
            let parity = (l >> 15 ^ l >> 12 ^ l >> 11 ^ l >> 10 ^ 1) & 1;
            self.noise_level = l << 1 | parity;
            self.noise_timer += 0x2_0000 >> shift;
            if self.noise_timer < 0 {
                self.noise_timer += 0x2_0000 >> shift;
            }
        }
    }

    /// One voice's next sample after its envelope, or None when silent.
    fn voice_sample(&mut self, v: usize, prev_out: i32) -> Option<i32> {
        if self.voices[v].phase == Phase::Off {
            self.voices[v].last_out = 0;
            return None;
        }
        if !self.voices[v].has_block {
            self.decode_block(v);
        }
        let base = (v as u32) << 4;
        let mut step = (self.reg(base + 4) as u32).min(0x4000);
        let pmon = self.reg(0x190) as u32 | (self.reg(0x192) as u32) << 16;
        if v > 0 && (pmon >> v) & 1 != 0 {
            let factor = (prev_out + 0x8000) as i64;
            step = ((step as i64 * factor) >> 15).clamp(0, 0x3FFF) as u32;
        }
        let non = self.reg(0x194) as u32 | (self.reg(0x196) as u32) << 16;
        let noise = (non >> v) & 1 != 0;
        let vc = &mut self.voices[v];
        // the Gaussian over samples i-3..i (the chip's interpolation)
        let i = (vc.counter >> 12) as usize;
        let f = (vc.counter >> 4 & 0xFF) as usize;
        let g = |k: usize, s: i16| GAUSS[k] as i32 * s as i32 >> 15;
        let interp = (g(0xFF - f, vc.buf[i]) + g(0x1FF - f, vc.buf[i + 1]) + g(0x100 + f, vc.buf[i + 2]) + g(f, vc.buf[i + 3])).clamp(-32768, 32767);
        let sample = if noise { self.noise_level as i16 as i32 } else { interp };
        let out = sample * vc.level >> 15;
        vc.last_out = out;
        vc.counter += step;
        if vc.counter >> 12 >= 28 {
            // block done: on to the next, or to the loop address / silence at an end flag
            vc.counter -= 28 << 12;
            vc.has_block = false;
            let flags = vc.block_flags;
            if flags & 1 != 0 {
                vc.addr = vc.repeat;
                if flags & 2 == 0 {
                    vc.phase = Phase::Off;
                    vc.level = 0;
                }
                self.endx |= 1 << v;
            } else {
                vc.addr = (vc.addr + 16) & (RAM_SIZE as u32 - 1);
            }
        }
        self.adsr(v);
        Some(out)
    }

    /// Reverb buffer sample at `off` bytes from the current position (wrapping in the buffer).
    fn rv_addr(&self, off: i64) -> Option<usize> {
        let base = self.reverb_addr as i64;
        let size = RAM_SIZE as i64 - base;
        if size <= 0 {
            return None;
        }
        let a = base + (self.reverb_pos as i64 - base + off).rem_euclid(size);
        Some(a as usize & (RAM_SIZE - 2))
    }
    fn rv(&self, off: i64) -> i32 {
        self.rv_addr(off).map_or(0, |a| i16::from_le_bytes([self.ram[a], self.ram[a + 1]]) as i32)
    }
    fn rv_write(&mut self, off: i64, v: i32) {
        if let Some(a) = self.rv_addr(off) {
            self.ram[a..a + 2].copy_from_slice(&(v.clamp(-32768, 32767) as i16).to_le_bytes());
        }
    }

    /// The reverb unit (22.05 kHz): both sides from the reverb input, per psx-spx.
    fn reverb(&mut self) {
        let regs: [u16; 32] = self.regs[0x1C0 >> 1..(0x1C0 >> 1) + 32].try_into().unwrap();
        let r = |i: usize| regs[i];
        let m = |i: usize| (r(i) as i64) << 3;
        let vol = |i: usize| r(i) as i16 as i32;
        let mul = |a: i32, b: i32| a * b >> 15;
        let (lin, rin) = (mul(self.reverb_in[0], vol(30)), mul(self.reverb_in[1], vol(31)));
        let (d_apf1, d_apf2) = (m(0), m(1));
        let (v_iir, v_wall, v_apf1, v_apf2) = (vol(2), vol(7), vol(8), vol(9));
        let enabled = self.spucnt() & 0x80 != 0;
        if enabled {
            for (target, input, src) in [(m(10), lin, m(16)), (m(11), rin, m(17)), (m(18), lin, m(25)), (m(19), rin, m(24))] {
                let prev = self.rv(target - 2);
                let v = mul(input + mul(self.rv(src), v_wall) - prev, v_iir) + prev;
                self.rv_write(target, v);
            }
        }
        let mut out = [0i32; 2];
        for side in 0..2 {
            let combs = [12, 14, 20, 22].map(|i| m(i + side));
            let mut o = (0..4).map(|k| mul(vol(3 + k), self.rv(combs[k]))).sum::<i32>().clamp(-32768, 32767);
            for (apf, d, v) in [(m(26 + side), d_apf1, v_apf1), (m(28 + side), d_apf2, v_apf2)] {
                let t = self.rv(apf - d);
                o = (o - mul(v, t)).clamp(-32768, 32767);
                if enabled {
                    self.rv_write(apf, o);
                }
                o = (mul(o, v) + t).clamp(-32768, 32767);
            }
            out[side] = mul(o, self.reg(0x184 + 2 * side as u32) as i16 as i32);
        }
        self.reverb_out = out;
        // on to the next buffer position
        self.reverb_pos += 2;
        if self.reverb_pos >= RAM_SIZE as u32 {
            self.reverb_pos = self.reverb_addr;
        }
    }

    /// The playing voices' state, one line each (diagnostics).
    pub fn describe(&self) -> String {
        let mut o = format!("spucnt {:04x} main {:04x}/{:04x} non {:04x}{:04x} pmon {:04x}{:04x} eon {:04x}{:04x} rv {:04x}/{:04x} base {:#x}
",
            self.spucnt(), self.reg(0x180), self.reg(0x182), self.reg(0x196), self.reg(0x194), self.reg(0x192), self.reg(0x190), self.reg(0x19A), self.reg(0x198), self.reg(0x184), self.reg(0x186), self.reverb_addr);
        for (i, v) in self.voices.iter().enumerate().filter(|(_, v)| v.phase != Phase::Off) {
            let b = (i as u32) << 4;
            o += &format!("  v{i:2} {:?} lvl {:5} vol {:04x}/{:04x} pitch {:04x} start {:#x} addr {:#x} rep {:#x} adsr {:04x}:{:04x} flags {:x}
",
                v.phase, v.level, self.reg(b), self.reg(b + 2), self.reg(b + 4), (self.reg(b + 6) as u32) << 3, v.addr, v.repeat, self.reg(b + 8), self.reg(b + 10), v.block_flags);
        }
        o
    }

    /// Produce `n` stereo samples into `out`.
    pub fn run(&mut self, n: usize) {
        self.out.reserve(n * 2);
        for _ in 0..n {
            self.noise_step();
            let (mut l, mut r) = (0i32, 0i32);
            let (mut rl, mut rr) = (0i32, 0i32);
            let eon = self.reg(0x198) as u32 | (self.reg(0x19A) as u32) << 16;
            let mut prev = 0;
            for v in 0..24 {
                let s = self.voice_sample(v, prev);
                prev = self.voices[v].last_out;
                let base = (v as u32) << 4;
                for side in 0..2 {
                    let reg = self.reg(base + 2 * side as u32);
                    let vc = &mut self.voices[v];
                    vc.vol[side] = Self::sweep(&mut vc.vol_env[side], vc.vol[side], reg);
                }
                let Some(s) = s else { continue };
                let vc = &self.voices[v];
                let (a, b) = (s * vc.vol[0] >> 15, s * vc.vol[1] >> 15);
                l += a;
                r += b;
                if eon >> v & 1 != 0 {
                    rl += a;
                    rr += b;
                }
            }
            // reverb at half rate
            self.reverb_in[0] = if self.odd { (self.reverb_in[0] + rl) >> 1 } else { rl };
            self.reverb_in[1] = if self.odd { (self.reverb_in[1] + rr) >> 1 } else { rr };
            if self.odd {
                self.reverb();
            }
            self.odd = !self.odd;
            for side in 0..2 {
                let reg = self.reg(0x180 + 2 * side as u32);
                self.main_vol[side] = Self::sweep(&mut self.main_env[side], self.main_vol[side], reg);
            }
            let cnt = self.spucnt();
            let (mut ol, mut or) = (0, 0);
            if cnt & 0x8000 != 0 && cnt & 0x4000 != 0 {
                ol = (l.clamp(-32768, 32767) * self.main_vol[0] >> 15) + self.reverb_out[0];
                or = (r.clamp(-32768, 32767) * self.main_vol[1] >> 15) + self.reverb_out[1];
            }
            self.out.push(ol.clamp(-32768, 32767) as i16);
            self.out.push(or.clamp(-32768, 32767) as i16);
        }
    }
}

impl IoBus for Spu {
    fn read(&mut self, a: u32, n: u32) -> u32 {
        if (SPU_BASE..SPU_BASE + 0x400).contains(&a) {
            let off = a - SPU_BASE;
            return match n {
                4 => self.read_reg(off) as u32 | (self.read_reg(off + 2) as u32) << 16,
                2 => self.read_reg(off) as u32,
                _ => (self.read_reg(off) >> ((a & 1) * 8)) as u32 & 0xFF,
            };
        }
        let word = match a & !3 {
            DMA4_MADR => self.dma4[0],
            DMA4_BCR => self.dma4[1],
            DMA4_CHCR => self.dma4[2],
            DPCR => self.dpcr,
            DICR => self.dicr,
            _ => 0,
        };
        word >> ((a & 3) * 8)
    }

    fn write(&mut self, a: u32, v: u32, n: u32, ram: &mut [u8]) {
        match a {
            DMA4_MADR => self.dma4[0] = v & 0xFF_FFFF,
            DMA4_BCR => self.dma4[1] = v,
            DMA4_CHCR => {
                self.dma4[2] = v;
                if v & 0x0100_0000 != 0 {
                    self.dma4(ram);
                }
            }
            DPCR => self.dpcr = v,
            DICR => {
                // flags (24..30) are acknowledged by writing 1
                let flags = self.dicr & 0x7F00_0000 & !(v & 0x7F00_0000);
                self.dicr = (v & 0x00FF_803F) | flags;
            }
            _ if (SPU_BASE..SPU_BASE + 0x400).contains(&a) => {
                let off = a - SPU_BASE;
                match n {
                    4 => {
                        self.write_reg(off, v as u16);
                        self.write_reg(off + 2, (v >> 16) as u16);
                    }
                    _ => self.write_reg(off, v as u16),
                }
            }
            _ => {}
        }
    }

    fn take_irqs(&mut self) -> u32 {
        std::mem::take(&mut self.irqs)
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
