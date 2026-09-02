pub(crate) use crate::header::{
    KSDATAFORMAT_SUBTYPE_AMBISONIC_IEEE_FLOAT, KSDATAFORMAT_SUBTYPE_AMBISONIC_PCM, W64_GUID_DATA,
    W64_GUID_FACT, W64_GUID_FMT, W64_GUID_RIFF, W64_GUID_WAVE, WAVE_FORMAT_ADPCM_IMA,
    WAVE_FORMAT_ADPCM_MS, WAVE_FORMAT_ALAW, WAVE_FORMAT_EXTENSIBLE, WAVE_FORMAT_IEEE_FLOAT,
    WAVE_FORMAT_MULAW, WAVE_FORMAT_PCM,
};

// --- deterministic RNG ---

pub struct XorShift64(u64);

impl XorShift64 {
    pub fn new(seed: u64) -> Self {
        XorShift64(seed | 1)
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn next_u8(&mut self) -> u8 {
        (self.next_u64() >> 59) as u8
    }
}

// --- codec matrix definition ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestCodec {
    U8,
    S16,
    S24,
    S32,
    F32,
    F64,
    ALaw,
    MuLaw,
}

impl TestCodec {
    pub const ALL: [TestCodec; 8] = [
        TestCodec::U8,
        TestCodec::S16,
        TestCodec::S24,
        TestCodec::S32,
        TestCodec::F32,
        TestCodec::F64,
        TestCodec::ALaw,
        TestCodec::MuLaw,
    ];

    pub fn fmt_tag(self) -> u16 {
        match self {
            TestCodec::U8 | TestCodec::S16 | TestCodec::S24 | TestCodec::S32 => WAVE_FORMAT_PCM,
            TestCodec::F32 | TestCodec::F64 => WAVE_FORMAT_IEEE_FLOAT,
            TestCodec::ALaw => WAVE_FORMAT_ALAW,
            TestCodec::MuLaw => WAVE_FORMAT_MULAW,
        }
    }

    pub fn bits(self) -> u16 {
        match self {
            TestCodec::U8 | TestCodec::ALaw | TestCodec::MuLaw => 8,
            TestCodec::S16 => 16,
            TestCodec::S24 => 24,
            TestCodec::S32 | TestCodec::F32 => 32,
            TestCodec::F64 => 64,
        }
    }

    pub fn width(self) -> usize {
        usize::from(self.bits() / 8)
    }

    pub fn ext_guid(self) -> [u8; 16] {
        match self {
            TestCodec::U8 | TestCodec::S16 | TestCodec::S24 | TestCodec::S32 => {
                crate::header::KSDATAFORMAT_SUBTYPE_PCM
            }
            TestCodec::F32 | TestCodec::F64 => crate::header::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
            TestCodec::ALaw => crate::header::KSDATAFORMAT_SUBTYPE_ALAW,
            TestCodec::MuLaw => crate::header::KSDATAFORMAT_SUBTYPE_MULAW,
        }
    }

    /// Hand-picked encoded samples covering the conversion extremes
    /// (min/max/-1/0/1, float specials incl. NaN payloads).
    pub fn extremes(self) -> Vec<Vec<u8>> {
        match self {
            TestCodec::U8 => vec![vec![0x00], vec![0x80], vec![0xFF], vec![0x7F], vec![0x01]],
            TestCodec::S16 => [0i16, -1, 1, i16::MIN, i16::MAX, 12345, -12345]
                .iter()
                .map(|v| v.to_le_bytes().to_vec())
                .collect(),
            TestCodec::S24 => [
                [0x00, 0x00, 0x00],
                [0xFF, 0xFF, 0xFF], // -1
                [0x00, 0x00, 0x80], // min
                [0xFF, 0xFF, 0x7F], // max
                [0x01, 0x00, 0x00],
                [0x56, 0x34, 0x12],
                [0xAA, 0xAA, 0xAA],
            ]
            .iter()
            .map(|v| v.to_vec())
            .collect(),
            TestCodec::S32 => [
                0i32,
                -1,
                1,
                i32::MIN,
                i32::MAX,
                0x1234_5678,
                -0x1234_5678,
                0x0000_FFFF,
                1 << 30,
            ]
            .iter()
            .map(|v| v.to_le_bytes().to_vec())
            .collect(),
            TestCodec::F32 => [
                0.0f32,
                -0.0,
                1.0,
                -1.0,
                std::f32::consts::PI,
                -2.718_281_7,
                1e30,
                -1e-30,
                f32::MAX,
                f32::MIN_POSITIVE,
                1e-40, // subnormal
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NAN,
                f32::from_bits(0x7FC0_0001), // NaN with payload
            ]
            .iter()
            .map(|v| v.to_le_bytes().to_vec())
            .collect(),
            TestCodec::F64 => [
                0.0f64,
                -0.0,
                1.0,
                -1.0,
                0.1,
                std::f64::consts::PI,
                1e300,
                -1e-300,
                f64::MAX,
                f64::MIN_POSITIVE,
                5e-324, // subnormal
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NAN,
                f64::from_bits(0x7FF8_0000_0000_0001), // NaN with payload
            ]
            .iter()
            .map(|v| v.to_le_bytes().to_vec())
            .collect(),
            TestCodec::ALaw | TestCodec::MuLaw => {
                vec![
                    vec![0x00],
                    vec![0x55],
                    vec![0x7F],
                    vec![0x80],
                    vec![0xD5],
                    vec![0xFF],
                ]
            }
        }
    }
}

pub fn gen_payload(
    codec: TestCodec,
    rng: &mut XorShift64,
    frames: usize,
    channels: usize,
) -> Vec<u8> {
    let extremes = codec.extremes();
    let total = frames * channels;
    let mut out = Vec::with_capacity(total * codec.width());
    for i in 0..total {
        if i < extremes.len() {
            out.extend_from_slice(&extremes[i]);
        } else {
            for _ in 0..codec.width() {
                out.push(rng.next_u8());
            }
        }
    }
    out
}

// --- WAV builder ---

pub struct WavBuilder {
    pub sample_rate: u32,
    pub channels: u16,
    pub codec: TestCodec,
    pub extensible: bool,
    pub valid_bits: Option<u16>,
    pub channel_mask: Option<u32>,
    /// fmt chunk length for plain PCM: 16, 18, or 40.
    pub pcm_fmt_len: u32,
    pub chunks_before_fmt: Vec<([u8; 4], Vec<u8>)>,
    pub chunks_before_data: Vec<([u8; 4], Vec<u8>)>,
    pub declared_data_len: Option<u32>,
    pub riff_len: Option<u32>,
    pub truncate_file: Option<usize>,
    pub payload: Vec<u8>,
}

impl WavBuilder {
    pub fn new(codec: TestCodec) -> Self {
        WavBuilder {
            sample_rate: 16000,
            channels: 1,
            codec,
            extensible: false,
            valid_bits: None,
            channel_mask: None,
            pcm_fmt_len: 16,
            chunks_before_fmt: Vec::new(),
            chunks_before_data: Vec::new(),
            declared_data_len: None,
            riff_len: None,
            truncate_file: None,
            payload: Vec::new(),
        }
    }

    pub fn fmt_body(&self) -> Vec<u8> {
        let width = self.codec.width() as u16;
        let block_align = self.channels * width;
        let byte_rate = self.sample_rate.wrapping_mul(u32::from(block_align));
        let tag = if self.extensible {
            WAVE_FORMAT_EXTENSIBLE
        } else {
            self.codec.fmt_tag()
        };
        let bits = self.codec.bits();

        let mut v = Vec::new();
        v.extend_from_slice(&tag.to_le_bytes());
        v.extend_from_slice(&self.channels.to_le_bytes());
        v.extend_from_slice(&self.sample_rate.to_le_bytes());
        v.extend_from_slice(&byte_rate.to_le_bytes());
        v.extend_from_slice(&block_align.to_le_bytes());
        v.extend_from_slice(&bits.to_le_bytes());

        if self.extensible {
            v.extend_from_slice(&22u16.to_le_bytes()); // cbSize
            v.extend_from_slice(&self.valid_bits.unwrap_or(bits).to_le_bytes());
            let mask = self.channel_mask.unwrap_or(if self.channels <= 18 {
                (1u32 << self.channels) - 1
            } else {
                0
            });
            v.extend_from_slice(&mask.to_le_bytes());
            v.extend_from_slice(&self.codec.ext_guid());
        } else {
            match self.codec.fmt_tag() {
                WAVE_FORMAT_PCM => match self.pcm_fmt_len {
                    16 => {}
                    18 => v.extend_from_slice(&0u16.to_le_bytes()),
                    40 => {
                        v.extend_from_slice(&22u16.to_le_bytes());
                        v.extend_from_slice(&[0u8; 22]);
                    }
                    _ => unreachable!("test builder: pcm_fmt_len must be 16/18/40"),
                },
                WAVE_FORMAT_ALAW | WAVE_FORMAT_MULAW => {
                    // cbSize = 0 -> canonical 18-byte g711 fmt chunk.
                    v.extend_from_slice(&0u16.to_le_bytes());
                }
                _ => {} // IEEE float: canonical 16 bytes
            }
        }
        v
    }

    pub fn build(&self) -> Vec<u8> {
        let mut body: Vec<u8> = Vec::new();
        let push_chunk = |body: &mut Vec<u8>, tag: [u8; 4], data: &[u8]| {
            body.extend_from_slice(&tag);
            body.extend_from_slice(&(data.len() as u32).to_le_bytes());
            body.extend_from_slice(data);
            if data.len() % 2 == 1 {
                body.push(0); // odd-byte padding
            }
        };

        for (tag, data) in &self.chunks_before_fmt {
            push_chunk(&mut body, *tag, data);
        }
        push_chunk(&mut body, *b"fmt ", &self.fmt_body());
        for (tag, data) in &self.chunks_before_data {
            push_chunk(&mut body, *tag, data);
        }

        body.extend_from_slice(b"data");
        let declared = self.declared_data_len.unwrap_or(self.payload.len() as u32);
        body.extend_from_slice(&declared.to_le_bytes());
        body.extend_from_slice(&self.payload);
        if self.payload.len() % 2 == 1 {
            body.push(0);
        }

        let riff_len = self.riff_len.unwrap_or(4 + body.len() as u32);
        let mut file = Vec::with_capacity(8 + body.len());
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&riff_len.to_le_bytes());
        file.extend_from_slice(b"WAVE");
        file.extend_from_slice(&body);
        if let Some(t) = self.truncate_file {
            file.truncate(t);
        }
        file
    }
}
